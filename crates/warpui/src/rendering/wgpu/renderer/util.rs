use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};

use wgpu::{
    util::BufferInitDescriptor, Buffer, BufferAddress, BufferDescriptor, BufferUsages, Device,
    Queue, COPY_BUFFER_ALIGNMENT,
};

use super::Error;

/// Whether the instance (per-rect, per-glyph, per-image) vertex buffers are kept across frames
/// and refilled with `Queue::write_buffer`, instead of a fresh mapped buffer being created and
/// dropped for each of them every frame. `PHOSPHOR_WGPU_PER_FRAME_BUFFERS=1` restores the
/// per-frame buffers, for bisecting a driver problem.
pub fn reuse_instance_buffers() -> bool {
    static REUSE: OnceLock<bool> = OnceLock::new();
    *REUSE.get_or_init(|| {
        reuse_unless_disabled(std::env::var("PHOSPHOR_WGPU_PER_FRAME_BUFFERS").ok().as_deref())
    })
}

fn reuse_unless_disabled(per_frame_buffers_flag: Option<&str>) -> bool {
    per_frame_buffers_flag.is_none_or(|flag| flag == "0")
}

/// Whether a frame whose base layer starts with a full-surface, opaque, solid rect clears the
/// surface to that colour and skips the rect, instead of clearing to transparent and filling the
/// whole surface a second time (#1232). `PHOSPHOR_WGPU_CLEAR_TO_BACKGROUND=0` restores the two
/// passes, for A/B measurement.
pub fn clear_to_background() -> bool {
    static CLEAR: OnceLock<bool> = OnceLock::new();
    *CLEAR.get_or_init(|| {
        std::env::var("PHOSPHOR_WGPU_CLEAR_TO_BACKGROUND")
            .ok()
            .is_none_or(|flag| flag != "0")
    })
}

/// A vertex buffer that outlives the frame. `write_buffer` is ordered on the queue after the
/// previous frame's submission, so refilling it while that frame's draws may still be executing
/// is safe by the wgpu queue-timeline rules.
struct PersistentBuffer {
    buffer: Buffer,
    capacity: BufferAddress,
}

/// The vertex buffer one pipeline draws its instances from in the current frame: either the
/// buffer kept across frames or, when [`reuse_instance_buffers`] is off, this frame's own.
#[derive(Default)]
pub struct InstanceBuffers {
    persistent: Option<PersistentBuffer>,
    this_frame: Option<Buffer>,
}

impl InstanceBuffers {
    pub fn begin_frame(&mut self) {
        self.this_frame = None;
    }

    /// Makes `contents` the instance data for this frame. On failure no buffer is available and
    /// the pipeline draws nothing this frame, as before.
    pub fn upload(
        &mut self,
        label: &'static str,
        device: &Device,
        queue: &Queue,
        device_lost: &Arc<AtomicBool>,
        contents: &[u8],
    ) {
        if reuse_instance_buffers() {
            if let Err(error) = upload_persistent(
                &mut self.persistent,
                label,
                device,
                queue,
                device_lost,
                contents,
            ) {
                log::warn!("Failed to upload {label}: {error:#}");
                self.persistent = None;
            }
        } else {
            self.this_frame = create_buffer_init(
                device,
                device_lost,
                &BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage: BufferUsages::VERTEX,
                },
            )
            .ok();
        }
    }

    pub fn vertex_buffer(&self) -> Option<&Buffer> {
        self.this_frame
            .as_ref()
            .or_else(|| self.persistent.as_ref().map(|persistent| &persistent.buffer))
    }
}

/// Writes `contents` into the persistent buffer in `slot`, creating or growing it first when it
/// is missing or too small. `contents.len()` must be a multiple of `COPY_BUFFER_ALIGNMENT`, which
/// the instance types guarantee (see the `const` asserts next to them).
fn upload_persistent(
    slot: &mut Option<PersistentBuffer>,
    label: &'static str,
    device: &Device,
    queue: &Queue,
    device_lost: &Arc<AtomicBool>,
    contents: &[u8],
) -> Result<(), Error> {
    let needed = (contents.len() as BufferAddress).max(COPY_BUFFER_ALIGNMENT);
    if slot
        .as_ref()
        .is_none_or(|persistent| persistent.capacity < needed)
    {
        let capacity = needed.next_power_of_two();
        let buffer = create_buffer(
            device,
            &BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        )?;
        if device_lost.load(Ordering::SeqCst) {
            return Err(Error::DeviceLost);
        }
        *slot = Some(PersistentBuffer { buffer, capacity });
    }
    if let Some(persistent) = slot.as_ref() {
        if !contents.is_empty() {
            queue.write_buffer(&persistent.buffer, 0, contents);
        }
    }
    Ok(())
}

/// Calls the provided function, capturing and returning any validation errors
/// detected by wgpu.
#[must_use]
pub fn with_error_scope<T>(
    device: &wgpu::Device,
    callback: impl FnOnce() -> T,
) -> (T, Option<Error>) {
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let ret = callback();
    // The future returned by `pop_error_scope` resolves immediately.
    let error_future = error_scope.pop();
    use futures::FutureExt;
    let error = error_future
        .now_or_never()
        .expect("always resolves immediately");
    (ret, error.map(Into::into))
}

/// Creates a buffer and initializes it with data, synchronously returning an
/// error if the buffer could not be created successfully.
///
/// This is adapted from [`wgpu::util::DeviceExt::create_buffer_init`], with
/// added logic to check for and return errors from the underlying buffer
/// creation.
pub fn create_buffer_init(
    device: &Device,
    device_lost: &Arc<AtomicBool>,
    descriptor: &BufferInitDescriptor<'_>,
) -> Result<Buffer, super::Error> {
    // Skip mapping if the buffer is zero sized
    if descriptor.contents.is_empty() {
        let wgt_descriptor = BufferDescriptor {
            label: descriptor.label,
            size: 0,
            usage: descriptor.usage,
            mapped_at_creation: false,
        };

        create_buffer(device, &wgt_descriptor)
    } else {
        let unpadded_size = descriptor.contents.len() as BufferAddress;
        // Valid vulkan usage is
        // 1. buffer size must be a multiple of COPY_BUFFER_ALIGNMENT.
        // 2. buffer size must be greater than 0.
        // Therefore we round the value up to the nearest multiple, and ensure it's at least COPY_BUFFER_ALIGNMENT.
        let align_mask = COPY_BUFFER_ALIGNMENT - 1;
        let padded_size = ((unpadded_size + align_mask) & !align_mask).max(COPY_BUFFER_ALIGNMENT);

        let wgt_descriptor = BufferDescriptor {
            label: descriptor.label,
            size: padded_size,
            usage: descriptor.usage,
            mapped_at_creation: true,
        };

        let buffer = create_buffer(device, &wgt_descriptor)?;

        if device_lost.load(Ordering::SeqCst) {
            return Err(super::Error::DeviceLost);
        }

        buffer
            .slice(..)
            .get_mapped_range_mut()
            .slice(..unpadded_size as usize)
            .copy_from_slice(descriptor.contents);
        buffer.unmap();

        Ok(buffer)
    }
}

/// Creates a buffer using the given device and descriptor, synchronously
/// returning an error if the buffer could not be created successfully.
fn create_buffer(device: &Device, desc: &BufferDescriptor<'_>) -> Result<Buffer, Error> {
    let (buffer, error) = with_error_scope(device, || device.create_buffer(desc));

    match error {
        Some(error) => {
            log::warn!("Failed to create wgpu::Buffer: {error:#}");
            Err(error)
        }
        None => Ok(buffer),
    }
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
