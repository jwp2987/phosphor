use anyhow::Result;
use base64::Engine;
use flate2::read::ZlibDecoder;
use pathfinder_geometry::vector::Vector2F;
use rand::Rng;
use std::cmp::min;
use std::io::Read;
#[cfg(feature = "local_fs")]
use std::{env, fs, path::Path, str};
use warpui::image_cache::{resize_dimensions, FitType};
use warpui::{
    image_cache::{CustomHeaderCreationError, CustomImageFormat, CustomImageHeader, ImageType},
    util::{parse_i32, parse_u32},
};

use super::escape_sequences::C1;

/// The largest width or height (`s=`/`v=`) accepted for a raw RGB/RGBA kitty
/// transmission. Matches kitty's own limit; anything larger is rejected before
/// any pixel data is read, decompressed or allocated (#876).
pub const MAX_KITTY_IMAGE_DIMENSION: u32 = 10_000;

/// The most image bytes a single kitty transmission may decode to, whatever
/// the medium (direct payload, zlib output, file, shared memory). Equal to
/// `MAX_DECODED_IMAGE_BYTES` (exactly an 8192x8192 RGBA image), which sits under
/// the asset cache's 320 MB budget, so an accepted image is never decoded only
/// to be evicted straight away (#880). Kitty's own `MAX_DATA_SZ` is 400 MB, so a
/// raw image between the two sizes that kitty accepts is refused here.
pub const MAX_KITTY_IMAGE_BYTES: usize = warpui::image_cache::MAX_DECODED_IMAGE_BYTES as usize;

/// The most base64 text one kitty transmission may accumulate across all its
/// chunks (and the most a single APC sequence may buffer) before the terminal
/// stops buffering and drops it: the base64 length of `MAX_KITTY_IMAGE_BYTES`,
/// plus a little slack for the control data (#877).
pub const MAX_KITTY_BASE64_BYTES: usize = MAX_KITTY_IMAGE_BYTES.div_ceil(3) * 4 + 4096;

/// Actions specified by the [Kitty Image Protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
#[derive(Debug, Clone)]
pub enum KittyAction {
    StoreOnly(StoreOnly),
    StoreAndDisplay(StoreAndDisplay),
    DisplayStoredImage(DisplayStoredImage),
    QuerySupport(QuerySupport),
    Delete {
        delete_placements_only: bool,
        deletion_type: DeletionType,
    },
}

#[derive(Debug, Default, Clone)]
pub struct StoreOnly {
    pub image: KittyImage,
    pub image_id: u32,
}

#[derive(Debug, Default, Clone)]
pub struct StoreAndDisplay {
    pub image: KittyImage,
    pub placement_data: KittyPlacementData,
    pub image_id: u32,
    pub placement_id: u32,
}

#[derive(Debug, Default, Clone)]
pub struct DisplayStoredImage {
    pub placement_data: KittyPlacementData,
    pub image_id: u32,
    pub placement_id: u32,
}

#[derive(Debug, Default, Clone)]
pub struct QuerySupport {
    pub image: KittyImage,
    pub image_id: u32,
}

#[derive(Debug, Clone)]
pub enum DeletionType {
    DeleteAll,
    DeleteById(DeleteById),
}

#[derive(Debug, Clone)]
pub struct DeleteById {
    pub image_id: u32,
    pub placement_id: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct KittyImage {
    pub metadata: KittyImageMetadata,
    pub data: Vec<u8>,
}

impl TryFrom<PendingKittyMessage> for KittyMessage {
    type Error = InvalidKittyPayload;

    fn try_from(pending: PendingKittyMessage) -> Result<KittyMessage, InvalidKittyPayload> {
        Ok(KittyMessage {
            payload: decode_base64_chunks(&pending.payload, MAX_KITTY_IMAGE_BYTES)?,
            control_data: pending.control_data,
        })
    }
}

/// Base64-decodes and concatenates a transmission's chunks, failing once the
/// total exceeds `limit` bytes.
fn decode_base64_chunks(chunks: &[Vec<u8>], limit: usize) -> Result<Vec<u8>, KittyDecodeError> {
    let mut decoded_payload = vec![];
    for chunk in chunks {
        let decoded = base64_decode_padding_agnostic(&chunk[..])?;
        if decoded_payload.len().saturating_add(decoded.len()) > limit {
            return Err(KittyDecodeError::PayloadTooLarge { limit });
        }
        decoded_payload.extend(decoded);
    }
    Ok(decoded_payload)
}

impl TryFrom<KittyMessage> for KittyImage {
    type Error = InvalidKittyPayload;

    fn try_from(message: KittyMessage) -> Result<Self, InvalidKittyPayload> {
        // Reject impossible raw-pixel dimensions before reading the medium,
        // decompressing, or allocating anything on their behalf (#876). A
        // direct payload has already been base64-decoded (under
        // MAX_KITTY_IMAGE_BYTES) by `KittyMessage::try_from`.
        let raw_byte_count = raw_pixel_byte_count(&message.control_data)?;

        // Only a direct payload is the image itself; the other media carry a
        // path, which is not bounded by the pixel count.
        let decoded_limit = match message.control_data.transmission_medium {
            KittyTransmissionMedium::Direct => raw_byte_count.unwrap_or(MAX_KITTY_IMAGE_BYTES),
            _ => MAX_KITTY_IMAGE_BYTES,
        };
        let decoded_data = decode_kitty_image_data_bounded(
            message.payload,
            message.control_data.compressed,
            decoded_limit,
        )?;

        let decoded_data = match message.control_data.transmission_medium {
            KittyTransmissionMedium::Direct => decoded_data,
            KittyTransmissionMedium::SimpleFile => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "local_fs")] {
                        read_file(decoded_data, false)?
                    } else {
                        return Err(InvalidKittyPayload::FileError(FileError::UnsupportedPlatform));
                    }
                }
            }
            KittyTransmissionMedium::TemporaryFile => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "local_fs")] {
                        read_file(decoded_data, true)?
                    } else {
                        return Err(InvalidKittyPayload::FileError(FileError::UnsupportedPlatform));
                    }
                }
            }
            KittyTransmissionMedium::SharedMemoryObject => {
                cfg_if::cfg_if! {
                    if #[cfg(all(feature = "local_fs", unix))] {
                        read_shared_memory(message.control_data.clone(), decoded_data)?
                    } else {
                        return Err(InvalidKittyPayload::ShmError(ShmError::UnsupportedPlatform));
                    }
                }
            }
        };

        Ok(Self {
            metadata: KittyImageMetadata::from(message.control_data),
            data: decoded_data,
        })
    }
}

pub type KittyResponse = Result<(), KittyError>;

#[derive(Debug, Clone)]
pub enum KittyError {
    InvalidKittyAction(InvalidKittyAction),
    StorageError(StorageError),
    KittyFeatureDisabled,
}

#[derive(Debug, Clone)]
pub enum StorageError {
    UnknownId { id: u32 },
}

impl From<StorageError> for KittyError {
    fn from(value: StorageError) -> Self {
        KittyError::StorageError(value)
    }
}

#[derive(Debug, Clone)]
pub enum InvalidKittyAction {
    UnsupportedAction,
    InvalidKittyPayload(InvalidKittyPayload),
    InvalidControlData(InvalidControlData),
}

impl From<InvalidKittyAction> for KittyError {
    fn from(value: InvalidKittyAction) -> Self {
        KittyError::InvalidKittyAction(value)
    }
}

#[derive(Debug, Clone)]
pub enum InvalidControlData {
    IdMissing,
    UnicodePlaceholderUnsupported,
}

impl From<InvalidControlData> for KittyError {
    fn from(value: InvalidControlData) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidControlData(value))
    }
}

#[derive(Debug, Clone)]
pub enum InvalidKittyPayload {
    InvalidTransmissionMedium(KittyTransmissionMedium),
    KittyDecodeError(KittyDecodeError),
    InvalidKittyImage(InvalidKittyImage),
    FileError(FileError),
    ShmError(ShmError),
}

#[derive(Debug, Clone)]
pub enum FileError {
    FileReadError(String),
    UnsupportedPlatform,
}

impl From<InvalidKittyPayload> for KittyError {
    fn from(value: InvalidKittyPayload) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidKittyPayload(value))
    }
}

#[derive(Debug, Clone)]
pub enum ShmError {
    ObjectOpenError(String),
    MmapError,
    EmptyObject,
    ObjectTooSmall {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    InvalidObjectSize,
    FileStatError(String),
    UnsupportedPlatform,
}

#[derive(Debug, Clone)]
pub enum KittyDecodeError {
    InvalidBase64(String),
    InvalidCompression(String),
    InvalidUtf8(String),
    /// The transmission decodes (or decompresses, or reads) to more than
    /// `limit` bytes.
    PayloadTooLarge {
        limit: usize,
    },
}

impl From<KittyDecodeError> for InvalidKittyPayload {
    fn from(value: KittyDecodeError) -> Self {
        InvalidKittyPayload::KittyDecodeError(value)
    }
}

impl From<KittyDecodeError> for KittyError {
    fn from(value: KittyDecodeError) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidKittyPayload(
            InvalidKittyPayload::KittyDecodeError(value),
        ))
    }
}

#[derive(Debug, Clone)]
pub enum InvalidKittyImage {
    KittyPngError(KittyPngError),
    KittyRgbError(KittyRgbError),
    MalformedImage,
}

impl From<InvalidKittyImage> for KittyError {
    fn from(value: InvalidKittyImage) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidKittyPayload(
            InvalidKittyPayload::InvalidKittyImage(value),
        ))
    }
}

#[derive(Debug, Clone)]
pub enum KittyPngError {
    InvalidBytes(String),
}

impl From<KittyPngError> for KittyError {
    fn from(value: KittyPngError) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidKittyPayload(
            InvalidKittyPayload::InvalidKittyImage(InvalidKittyImage::KittyPngError(value)),
        ))
    }
}

#[derive(Debug, Clone)]
pub enum KittyRgbError {
    ExpectedDataSizeMismatch {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    /// `s=`/`v=` exceed `MAX_KITTY_IMAGE_DIMENSION`, or their pixel byte count
    /// is not addressable.
    DimensionsTooLarge { width: u32, height: u32 },
}

impl From<KittyRgbError> for KittyError {
    fn from(value: KittyRgbError) -> Self {
        KittyError::InvalidKittyAction(InvalidKittyAction::InvalidKittyPayload(
            InvalidKittyPayload::InvalidKittyImage(InvalidKittyImage::KittyRgbError(value)),
        ))
    }
}

impl From<CustomHeaderCreationError> for KittyRgbError {
    fn from(value: CustomHeaderCreationError) -> Self {
        match value {
            CustomHeaderCreationError::ExpectedDataSizeMismatch {
                expected_bytes,
                actual_bytes,
            } => KittyRgbError::ExpectedDataSizeMismatch {
                expected_bytes,
                actual_bytes,
            },
            CustomHeaderCreationError::DimensionsTooLarge { width, height } => {
                KittyRgbError::DimensionsTooLarge { width, height }
            }
        }
    }
}

impl TryFrom<KittyMessage> for KittyAction {
    type Error = KittyError;

    fn try_from(message: KittyMessage) -> Result<KittyAction, KittyError> {
        match message.control_data.placement_action {
            KittyPlacementAction::StoreOnly => {
                let mut action = StoreOnly {
                    image_id: message
                        .control_data
                        .image_id
                        .unwrap_or(rand::thread_rng().r#gen()),
                    image: KittyImage::try_from(message)?,
                };

                if action.image.metadata.pixel_data_format == KittyPixelDataFormat::Png {
                    action.image = set_kitty_png_size(action.image)?;
                } else {
                    action.image = set_kitty_rgb_headers(action.image)?;
                }

                Ok(KittyAction::StoreOnly(action))
            }
            KittyPlacementAction::StoreAndDisplay => {
                if message.control_data.unicode_placeholder {
                    return Err(InvalidControlData::UnicodePlaceholderUnsupported.into());
                }

                let mut action = StoreAndDisplay {
                    image_id: message
                        .control_data
                        .image_id
                        .unwrap_or(rand::thread_rng().r#gen()),
                    placement_id: message
                        .control_data
                        .placement_id
                        .unwrap_or(rand::thread_rng().r#gen()),
                    placement_data: KittyPlacementData {
                        z_index: message.control_data.z_index,
                        cols: message.control_data.cols,
                        rows: message.control_data.rows,
                        cursor_movement_policy: message.control_data.cursor_movement_policy,
                    },
                    image: KittyImage::try_from(message)?,
                };

                if action.image.metadata.pixel_data_format == KittyPixelDataFormat::Png {
                    action.image = set_kitty_png_size(action.image)?;
                } else {
                    action.image = set_kitty_rgb_headers(action.image)?;
                }

                Ok(KittyAction::StoreAndDisplay(action))
            }
            KittyPlacementAction::DisplayStoredImage => {
                if message.control_data.unicode_placeholder {
                    return Err(InvalidControlData::UnicodePlaceholderUnsupported.into());
                }

                let id = match message.control_data.image_id {
                    Some(id) => id,
                    None => return Err(InvalidControlData::IdMissing.into()),
                };

                Ok(KittyAction::DisplayStoredImage(DisplayStoredImage {
                    image_id: id,
                    placement_id: message
                        .control_data
                        .placement_id
                        .unwrap_or(rand::thread_rng().r#gen()),
                    placement_data: KittyPlacementData {
                        z_index: message.control_data.z_index,
                        cols: message.control_data.cols,
                        rows: message.control_data.rows,
                        cursor_movement_policy: message.control_data.cursor_movement_policy,
                    },
                }))
            }
            KittyPlacementAction::QuerySupport => {
                let mut action = QuerySupport {
                    image_id: message
                        .control_data
                        .image_id
                        .unwrap_or(rand::thread_rng().r#gen()),
                    image: KittyImage::try_from(message)?,
                };

                if action.image.metadata.pixel_data_format == KittyPixelDataFormat::Png {
                    action.image = set_kitty_png_size(action.image)?;
                } else {
                    action.image = set_kitty_rgb_headers(action.image)?;
                }

                Ok(KittyAction::QuerySupport(action))
            }
            KittyPlacementAction::Delete => {
                let deletion_type = match message.control_data.delete_action {
                    DeleteAction::DeleteAll => DeletionType::DeleteAll,
                    DeleteAction::DeleteById => {
                        let image_id = match message.control_data.image_id {
                            Some(image_id) => image_id,
                            None => return Err(InvalidControlData::IdMissing.into()),
                        };

                        DeletionType::DeleteById(DeleteById {
                            image_id,
                            placement_id: message.control_data.placement_id,
                        })
                    }
                };

                Ok(KittyAction::Delete {
                    deletion_type,
                    delete_placements_only: message.control_data.delete_placements_only,
                })
            }
            KittyPlacementAction::Unknown => Err(InvalidKittyAction::UnsupportedAction.into()),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct KittyImageMetadata {
    pub pixel_data_format: KittyPixelDataFormat,
    pub transmission_medium: KittyTransmissionMedium,
    pub image_size: Vector2F,
}

#[derive(Debug, Default, Clone)]
pub struct KittyPlacementData {
    pub z_index: i32,
    pub cols: Option<u32>,
    pub rows: Option<u32>,
    pub cursor_movement_policy: CursorMovementPolicy,
}

impl KittyPlacementData {
    pub fn get_desired_dimensions(
        &self,
        image_size: Vector2F,
        cell_height: usize,
        cell_width: usize,
        max_width_px: usize,
        max_height_px: usize,
    ) -> Vector2F {
        let aspect_ratio = image_size.x() / image_size.y();

        let (width_px, height_px) = resize_dimensions(
            image_size.x() as u32,
            image_size.y() as u32,
            min(image_size.x() as u32, max_width_px as u32),
            min(image_size.y() as u32, max_height_px as u32),
            FitType::Contain,
        );

        let (desired_width_px, desired_height_px) = match (self.cols, self.rows) {
            (Some(width), None) => {
                let desired_width_px = (cell_width as u32).saturating_mul(width) as f32;
                (desired_width_px, desired_width_px / aspect_ratio)
            }
            (None, Some(height)) => {
                let desired_height_px = (cell_height as u32).saturating_mul(height) as f32;
                (desired_height_px * aspect_ratio, desired_height_px)
            }
            (Some(width), Some(height)) => (
                (cell_width as u32).saturating_mul(width) as f32,
                (cell_height as u32).saturating_mul(height) as f32,
            ),
            (None, None) => (width_px as f32, height_px as f32),
        };

        // `c=`/`r=` are untrusted u32s. Unclamped, `r=4294967295` became a
        // placement ~2.7e8 cells tall, and the caller emits one newline per
        // cell of height (#876). Fit inside the bounds the (None, None) case
        // already respects; when only one of `c`/`r` was given the aspect
        // ratio is the point, so scale both, otherwise clamp each.
        let max_width_px = max_width_px.max(1) as f32;
        let max_height_px = max_height_px.max(1) as f32;
        let (desired_width_px, desired_height_px) = match (self.cols, self.rows) {
            (Some(_), None) | (None, Some(_)) => {
                let scale = (max_width_px / desired_width_px)
                    .min(max_height_px / desired_height_px)
                    .min(1.0);
                (desired_width_px * scale, desired_height_px * scale)
            }
            _ => (desired_width_px, desired_height_px),
        };
        let clamp = |value: f32, max: f32| {
            if value.is_nan() {
                1.0
            } else {
                value.clamp(1.0, max)
            }
        };

        Vector2F::new(
            clamp(desired_width_px, max_width_px),
            clamp(desired_height_px, max_height_px),
        )
    }
}

impl From<KittyControlData> for KittyImageMetadata {
    fn from(control_data: KittyControlData) -> Self {
        Self {
            pixel_data_format: control_data.pixel_data_format,
            image_size: Vector2F::new(control_data.width as f32, control_data.height as f32),
            transmission_medium: control_data.transmission_medium,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KittyChunk {
    pub control_data: KittyControlData,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PendingKittyMessage {
    pub control_data: KittyControlData,
    pub payload: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct KittyMessage {
    pub control_data: KittyControlData,
    pub payload: Vec<u8>,
}

#[derive(Default, Debug, Clone)]
pub enum KittyPlacementAction {
    #[default]
    StoreOnly,
    StoreAndDisplay,
    DisplayStoredImage,
    QuerySupport,
    Delete,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct KittyControlData {
    pixel_data_format: KittyPixelDataFormat,
    width: u32,
    height: u32,
    pub compressed: bool,
    transmission_medium: KittyTransmissionMedium,
    pub further_chunks: bool,
    pub image_id: Option<u32>,
    pub placement_id: Option<u32>,
    pub placement_action: KittyPlacementAction,
    pub verbosity: KittyResponseVerbosity,
    pub z_index: i32,
    pub delete_action: DeleteAction,
    delete_placements_only: bool,
    pub rows: Option<u32>,
    pub cols: Option<u32>,
    pub cursor_movement_policy: CursorMovementPolicy,
    pub unicode_placeholder: bool,
}

impl Default for KittyControlData {
    fn default() -> Self {
        Self {
            pixel_data_format: KittyPixelDataFormat::default(),
            width: 0,
            height: 0,
            compressed: false,
            transmission_medium: KittyTransmissionMedium::default(),
            further_chunks: false,
            image_id: None,
            placement_id: None,
            placement_action: KittyPlacementAction::default(),
            verbosity: KittyResponseVerbosity::default(),
            z_index: 0,
            delete_action: DeleteAction::default(),
            delete_placements_only: true,
            rows: None,
            cols: None,
            cursor_movement_policy: CursorMovementPolicy::default(),
            unicode_placeholder: false,
        }
    }
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub enum DeleteAction {
    #[default]
    DeleteAll,
    DeleteById,
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub enum KittyPixelDataFormat {
    Rgb24Bit,
    #[default]
    Rgba32Bit,
    Png,
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub enum KittyTransmissionMedium {
    #[default]
    Direct,
    SimpleFile,
    TemporaryFile,
    SharedMemoryObject,
}

#[derive(Default, Debug, Clone, Copy)]
pub enum KittyResponseVerbosity {
    #[default]
    All,
    ErrorsOnly,
    None,
}

impl KittyResponseVerbosity {
    pub fn send_ok(&self) -> bool {
        matches!(self, KittyResponseVerbosity::All)
    }

    pub fn send_error(&self) -> bool {
        matches!(self, KittyResponseVerbosity::All)
            || matches!(self, KittyResponseVerbosity::ErrorsOnly)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum CursorMovementPolicy {
    #[default]
    MoveCursor,
    DoNotMoveCursor,
}

fn parse_kitty_control_data(control_data: &[u8]) -> KittyControlData {
    let params: Vec<&[u8]> = control_data.split(|&byte| byte == b',').collect();

    let mut parsed_control_data = KittyControlData::default();
    for param in params {
        let (key, value) = match param.iter().position(|&byte| byte == b'=') {
            Some(position) => (&param[..position], &param[position + 1..]),
            None => continue,
        };

        match key {
            b"f" => {
                parsed_control_data.pixel_data_format = match value {
                    b"24" => KittyPixelDataFormat::Rgb24Bit,
                    b"32" => KittyPixelDataFormat::Rgba32Bit,
                    b"100" => KittyPixelDataFormat::Png,
                    _ => KittyPixelDataFormat::default(),
                };
            }
            b"s" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.width = value;
                }
            }
            b"v" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.height = value;
                }
            }
            b"o" => {
                parsed_control_data.compressed = value == b"z";
            }
            b"m" => {
                parsed_control_data.further_chunks = value == b"1";
            }
            b"t" => {
                parsed_control_data.transmission_medium = match value {
                    b"d" => KittyTransmissionMedium::Direct,
                    b"f" => KittyTransmissionMedium::SimpleFile,
                    b"t" => KittyTransmissionMedium::TemporaryFile,
                    b"s" => KittyTransmissionMedium::SharedMemoryObject,
                    _ => KittyTransmissionMedium::default(),
                }
            }
            b"i" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.image_id = Some(value);
                }
            }
            b"p" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.placement_id = Some(value);
                }
            }
            b"a" => {
                parsed_control_data.placement_action = match value {
                    b"t" => KittyPlacementAction::StoreOnly,
                    b"T" => KittyPlacementAction::StoreAndDisplay,
                    b"p" => KittyPlacementAction::DisplayStoredImage,
                    b"q" => KittyPlacementAction::QuerySupport,
                    b"d" => KittyPlacementAction::Delete,
                    _ => KittyPlacementAction::Unknown,
                }
            }
            b"q" => {
                parsed_control_data.verbosity = match value {
                    b"0" => KittyResponseVerbosity::All,
                    b"1" => KittyResponseVerbosity::ErrorsOnly,
                    b"2" => KittyResponseVerbosity::None,
                    _ => KittyResponseVerbosity::default(),
                }
            }
            b"z" => {
                if let Some(value) = parse_i32(value) {
                    parsed_control_data.z_index = value;
                }
            }
            b"d" => {
                if value.iter().all(|x| x.is_ascii_uppercase()) {
                    parsed_control_data.delete_placements_only = false;
                }

                parsed_control_data.delete_action = match &value.to_ascii_lowercase()[..] {
                    // Note: Kitty Protocol specifies "a" as "Delete all placements on screen",
                    // we're just starting with DeleteAll placements to expedite the launch.
                    b"a" => DeleteAction::DeleteAll,
                    b"i" => DeleteAction::DeleteById,
                    _ => DeleteAction::default(),
                }
            }
            b"c" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.cols = Some(value);
                }
            }
            b"r" => {
                if let Some(value) = parse_u32(value) {
                    parsed_control_data.rows = Some(value);
                }
            }
            b"C" => {
                parsed_control_data.cursor_movement_policy = match value {
                    b"0" => CursorMovementPolicy::MoveCursor,
                    b"1" => CursorMovementPolicy::DoNotMoveCursor,
                    _ => CursorMovementPolicy::default(),
                }
            }
            b"U" => {
                parsed_control_data.unicode_placeholder = value == b"1";
            }
            _ => {}
        }
    }

    parsed_control_data
}

pub fn parse_kitty_chunk(chunk: &[u8]) -> KittyChunk {
    let (control_data, payload) = match chunk.iter().position(|&byte| byte == b';') {
        Some(position) => (&chunk[..position], &chunk[position + 1..]),
        None => (chunk, &[][..]),
    };

    let control_data = parse_kitty_control_data(control_data);

    KittyChunk {
        control_data,
        payload: payload.to_vec(),
    }
}

/// The longest the pty/model thread waits for a `t=f` / `t=t` file to be
/// validated and read (#824). The read is size-capped and cannot block on a
/// FIFO, but a file on a hung network mount still can.
#[cfg(feature = "local_fs")]
const KITTY_FILE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Most detached `kitty-file-read` helpers allowed to be alive at once. A
/// helper stuck on a hung mount never exits, so without a cap a program that
/// keeps printing `t=f` sequences leaks one thread per sequence (#824).
#[cfg(feature = "local_fs")]
const MAX_IN_FLIGHT_FILE_READS: usize = 8;

/// Helpers currently alive (running or stuck).
#[cfg(feature = "local_fs")]
static IN_FLIGHT_FILE_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Claims a slot in `counter` if fewer than `max` are taken.
#[cfg(feature = "local_fs")]
fn try_acquire_slot(counter: &std::sync::atomic::AtomicUsize, max: usize) -> bool {
    use std::sync::atomic::Ordering;
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < max).then_some(n + 1)
        })
        .is_ok()
}

/// Releases the in-flight slot when the helper finishes, even on panic.
#[cfg(feature = "local_fs")]
struct SlotGuard(&'static std::sync::atomic::AtomicUsize);

#[cfg(feature = "local_fs")]
impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Runs `work` on a helper thread and waits at most `timeout` for its result.
/// On timeout the helper is detached (it finishes, or stays stuck, on its own
/// without holding up the caller) and `None` is returned. `None` is also
/// returned, without spawning, when too many helpers are already in flight.
#[cfg(feature = "local_fs")]
fn run_with_timeout<T: Send + 'static>(
    timeout: std::time::Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    if !try_acquire_slot(&IN_FLIGHT_FILE_READS, MAX_IN_FLIGHT_FILE_READS) {
        return None;
    }
    let guard = SlotGuard(&IN_FLIGHT_FILE_READS);
    let (sender, receiver) = std::sync::mpsc::channel();
    // If the spawn fails the closure (and its guard) is dropped, releasing the slot.
    std::thread::Builder::new()
        .name("kitty-file-read".to_owned())
        .spawn(move || {
            let _guard = guard;
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.recv_timeout(timeout).ok()
}

/// Reads the file a `t=f` / `t=t` transmission names, off the pty/model thread
/// and with a deadline, so a stalled filesystem cannot freeze the terminal (#824).
#[cfg(feature = "local_fs")]
fn read_file(decoded_payload: Vec<u8>, is_temp: bool) -> Result<Vec<u8>, InvalidKittyPayload> {
    read_file_with_timeout(decoded_payload, is_temp, KITTY_FILE_READ_TIMEOUT)
}

#[cfg(feature = "local_fs")]
fn read_file_with_timeout(
    decoded_payload: Vec<u8>,
    is_temp: bool,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, InvalidKittyPayload> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    if IN_FLIGHT_FILE_READS.load(Ordering::Acquire) >= MAX_IN_FLIGHT_FILE_READS {
        return Err(InvalidKittyPayload::FileError(FileError::FileReadError(
            "too many file reads are stuck; refusing another".to_owned(),
        )));
    }
    // Set once the caller has stopped waiting; a late helper must not then
    // delete a `t=t` temp file nobody will ever use the data from.
    let abandoned = Arc::new(AtomicBool::new(false));
    let helper_abandoned = abandoned.clone();
    run_with_timeout(timeout, move || {
        read_file_blocking(decoded_payload, is_temp, &helper_abandoned)
    })
    .unwrap_or_else(|| {
        abandoned.store(true, Ordering::Release);
        Err(InvalidKittyPayload::FileError(FileError::FileReadError(
            "timed out reading the file".to_owned(),
        )))
    })
}

#[cfg(feature = "local_fs")]
fn read_file_blocking(
    decoded_payload: Vec<u8>,
    is_temp: bool,
    abandoned: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, InvalidKittyPayload> {
    let path = match str::from_utf8(&decoded_payload[..]) {
        Ok(path) => path,
        Err(err) => return Err(KittyDecodeError::InvalidUtf8(err.to_string()).into()),
    };

    // The path is chosen by whatever printed the escape sequence (#824), so
    // mirror kitty's own restrictions before opening anything: a regular file
    // outside /proc, /sys and /dev, and for `t=t` one that is in a temp dir and
    // carries the protocol's marker. Opening a FIFO would block forever.
    let path = validate_kitty_file_path(path, is_temp)?;
    let path = path.as_str();

    // Bounded: an unbounded `fs::read` of e.g. a huge file never returns.
    let data = match read_at_most(open_regular_file(path), MAX_KITTY_IMAGE_BYTES) {
        Ok(Some(data)) => data,
        Ok(None) => {
            return Err(KittyDecodeError::PayloadTooLarge {
                limit: MAX_KITTY_IMAGE_BYTES,
            }
            .into());
        }
        Err(err) => {
            return Err(InvalidKittyPayload::FileError(FileError::FileReadError(
                err.to_string(),
            )))
        }
    };

    if is_temp && !abandoned.load(std::sync::atomic::Ordering::Acquire) {
        safe_delete_temp_file(path);
    }

    Ok(data)
}

/// Directories a kitty `t=f` / `t=t` path may never resolve into: they hold
/// device nodes and pseudo-files that are unbounded or block on read.
#[cfg(feature = "local_fs")]
const KITTY_FORBIDDEN_PATH_PREFIXES: [&str; 3] = ["/proc", "/sys", "/dev"];

/// Resolves symlinks in `path` and checks it is something we may read: not
/// under a forbidden prefix, a regular file (not a FIFO, device or directory),
/// and, for `t=t`, a temp-dir path containing `tty-graphics-protocol`.
/// Returns the canonical path to read.
#[cfg(feature = "local_fs")]
fn validate_kitty_file_path(path: &str, is_temp: bool) -> Result<String, InvalidKittyPayload> {
    let file_error = |message: &str| {
        InvalidKittyPayload::FileError(FileError::FileReadError(message.to_owned()))
    };

    let canonical = fs::canonicalize(path)
        .map_err(|err| InvalidKittyPayload::FileError(FileError::FileReadError(err.to_string())))?;
    // Everything below tests the canonical path, never the raw one: a raw
    // `/tmp/tty-graphics-protocol/../../etc/x` would otherwise pass.
    let canonical_path = canonical;
    let canonical = canonical_path
        .to_str()
        .ok_or_else(|| file_error("path is not valid UTF-8"))?
        .to_owned();
    let canonical_path = canonical_path.as_path();

    // `/dev/shm` is a legitimate temp dir despite being under /dev.
    let forbidden = KITTY_FORBIDDEN_PATH_PREFIXES
        .iter()
        .any(|prefix| canonical_path.starts_with(prefix));
    if forbidden && !(is_temp && canonical_path.starts_with("/dev/shm")) {
        return Err(file_error("path is in a forbidden directory"));
    }

    if is_temp && !is_kitty_temp_path(canonical_path) {
        return Err(file_error(
            "temporary file must be in a temp dir and named for the graphics protocol",
        ));
    }

    match fs::metadata(&canonical) {
        Ok(metadata) if metadata.is_file() => Ok(canonical),
        Ok(_) => Err(file_error("not a regular file")),
        Err(err) => Err(file_error(&err.to_string())),
    }
}

/// Deletes a `t=t` file after it was read. `path` must be the canonical path
/// returned by `validate_kitty_file_path`; it is re-checked here regardless.
#[cfg(feature = "local_fs")]
fn safe_delete_temp_file(path: &str) {
    let Ok(canonical) = fs::canonicalize(path) else {
        return;
    };
    if is_kitty_temp_path(&canonical) {
        if let Err(err) = fs::remove_file(&canonical) {
            log::error!("Failed to delete kitty temporary file (path = {path}): {err}");
        }
    }
}

/// Opens `path` without following a final symlink or blocking on a FIFO, then
/// checks via fstat on the handle that it is a regular file. This closes the
/// window between validation and open (#824).
#[cfg(feature = "local_fs")]
fn open_regular_file(path: &str) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    Ok(file)
}

/// True if `canonical` lies inside a temp root and some component of it
/// carries the `tty-graphics-protocol` marker. `canonical` must already be
/// canonicalised; roots are canonicalised too (`/tmp` is `/private/tmp` on
/// macOS).
#[cfg(feature = "local_fs")]
fn is_kitty_temp_path(canonical: &Path) -> bool {
    is_path_in_temp_dir(canonical)
        && canonical.components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .contains("tty-graphics-protocol")
        })
}

#[cfg(feature = "local_fs")]
fn is_path_in_temp_dir(canonical: &Path) -> bool {
    let mut temp_dirs = vec!["/tmp".into(), "/var/tmp".into(), "/dev/shm".into()];
    // An empty `$TMPDIR` would make `starts_with("")` match everything.
    if let Some(temp_dir) = env::var_os("TMPDIR").filter(|dir| !dir.is_empty()) {
        temp_dirs.push(std::path::PathBuf::from(temp_dir));
    }

    temp_dirs.into_iter().any(|dir: std::path::PathBuf| {
        let dir = fs::canonicalize(&dir).unwrap_or(dir);
        dir.has_root() && dir.components().count() > 1 && canonical.starts_with(&dir)
    })
}

#[cfg(all(feature = "local_fs", unix))]
fn read_shared_memory(
    control_data: KittyControlData,
    decoded_payload: Vec<u8>,
) -> Result<Vec<u8>, InvalidKittyPayload> {
    use nix::sys::mman::{shm_open, shm_unlink};

    let path = match str::from_utf8(&decoded_payload[..]) {
        Ok(path) => path,
        Err(err) => return Err(KittyDecodeError::InvalidUtf8(err.to_string()).into()),
    };

    let fd = match shm_open(
        path,
        nix::fcntl::OFlag::O_RDONLY,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(err) => {
            return Err(InvalidKittyPayload::ShmError(ShmError::ObjectOpenError(
                err.to_string(),
            )))
        }
    };

    let data =
        raw_pixel_byte_count(&control_data).and_then(|size| read_from_shared_memory_fd(fd, size));

    if let Err(err) = shm_unlink(path) {
        log::warn!("Failed to unlink kitty shm file (path = {path}): {err:?}");
    };

    data
}

#[cfg(all(feature = "local_fs", unix))]
fn read_from_shared_memory_fd(
    fd: i32,
    size: Option<usize>,
) -> Result<Vec<u8>, InvalidKittyPayload> {
    use nix::sys::{
        mman::{mmap, MapFlags, ProtFlags},
        stat::fstat,
    };
    use std::num::NonZero;

    let file_size = match fstat(fd) {
        Ok(stat) => stat.st_size,
        Err(err) => {
            return Err(InvalidKittyPayload::ShmError(ShmError::FileStatError(
                err.to_string(),
            )))
        }
    };

    let Ok(file_size) = usize::try_from(file_size) else {
        return Err(InvalidKittyPayload::ShmError(ShmError::InvalidObjectSize));
    };

    let size = size.unwrap_or(file_size);

    if size > MAX_KITTY_IMAGE_BYTES {
        return Err(KittyDecodeError::PayloadTooLarge {
            limit: MAX_KITTY_IMAGE_BYTES,
        }
        .into());
    }

    if file_size < size {
        return Err(InvalidKittyPayload::ShmError(ShmError::ObjectTooSmall {
            expected_bytes: size,
            actual_bytes: file_size,
        }));
    }

    let Some(size) = NonZero::new(size) else {
        return Err(InvalidKittyPayload::ShmError(ShmError::EmptyObject));
    };

    let ptr = unsafe {
        mmap(
            None,
            size,
            ProtFlags::PROT_READ,
            MapFlags::MAP_SHARED,
            fd,
            0,
        )
    };

    let Ok(ptr) = ptr else {
        return Err(InvalidKittyPayload::ShmError(ShmError::MmapError));
    };

    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, size.into()) };
    let data = slice.to_vec();

    Ok(data)
}

fn base64_decode_padding_agnostic(data: &[u8]) -> Result<Vec<u8>, KittyDecodeError> {
    if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
        return Ok(decoded_bytes);
    }

    match base64::engine::general_purpose::STANDARD_NO_PAD.decode(data) {
        Ok(decoded_bytes) => Ok(decoded_bytes),
        Err(err) => Err(KittyDecodeError::InvalidBase64(err.to_string())),
    }
}

pub fn decode_kitty_image_data(
    payload: Vec<u8>,
    compressed: bool,
) -> Result<Vec<u8>, KittyDecodeError> {
    decode_kitty_image_data_bounded(payload, compressed, MAX_KITTY_IMAGE_BYTES)
}

/// `decode_kitty_image_data`, failing with `PayloadTooLarge` once the result
/// would exceed `limit` bytes. For a raw RGB/RGBA transmission the limit is the
/// exact byte count `s`/`v` describe, so zlib stops inflating as soon as it
/// has produced more than the image can hold.
fn decode_kitty_image_data_bounded(
    payload: Vec<u8>,
    compressed: bool,
    limit: usize,
) -> Result<Vec<u8>, KittyDecodeError> {
    let mut result = payload;

    // An uncompressed payload is already in memory, and one whose size is
    // merely wrong is reported later as `ExpectedDataSizeMismatch`; `limit`
    // only bounds what decompression may produce. (A compressed payload can
    // legitimately be larger than its pixels for tiny images.)
    if result.len() > MAX_KITTY_IMAGE_BYTES {
        return Err(KittyDecodeError::PayloadTooLarge {
            limit: MAX_KITTY_IMAGE_BYTES,
        });
    }

    if compressed {
        // Bounded: a few KB of zlib can inflate to gigabytes.
        let decompressed = read_at_most(Ok(ZlibDecoder::new(&result[..])), limit);
        result = match decompressed {
            Ok(Some(decompressed_bytes)) => decompressed_bytes,
            Ok(None) => return Err(KittyDecodeError::PayloadTooLarge { limit }),
            Err(err) => return Err(KittyDecodeError::InvalidCompression(err.to_string())),
        };
    }

    Ok(result)
}

/// Reads `reader` to the end, or returns `Ok(None)` as soon as it yields more
/// than `limit` bytes, so an oversized source never gets fully buffered.
fn read_at_most<R: Read>(
    reader: std::io::Result<R>,
    limit: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut data = vec![];
    let read_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    reader?.take(read_limit).read_to_end(&mut data)?;
    Ok((data.len() <= limit).then_some(data))
}

/// The byte count a raw RGB/RGBA transmission's `s=`/`v=` describe, or `None`
/// for PNG (whose dimensions come from the image itself). Errors if either
/// dimension exceeds `MAX_KITTY_IMAGE_DIMENSION` (#876: these are untrusted
/// `u32`s, and their product used to be computed in `u32` and overflow).
fn raw_pixel_byte_count(
    control_data: &KittyControlData,
) -> Result<Option<usize>, InvalidKittyPayload> {
    let image_format = match control_data.pixel_data_format {
        KittyPixelDataFormat::Rgb24Bit => CustomImageFormat::Rgb,
        KittyPixelDataFormat::Rgba32Bit => CustomImageFormat::Rgba,
        KittyPixelDataFormat::Png => return Ok(None),
    };
    let (width, height) = (control_data.width, control_data.height);
    let too_large = || {
        InvalidKittyPayload::InvalidKittyImage(InvalidKittyImage::KittyRgbError(
            KittyRgbError::DimensionsTooLarge { width, height },
        ))
    };
    if width > MAX_KITTY_IMAGE_DIMENSION || height > MAX_KITTY_IMAGE_DIMENSION {
        return Err(too_large());
    }
    match image_format.expected_byte_count(width, height) {
        Some(count) if count <= MAX_KITTY_IMAGE_BYTES => Ok(Some(count)),
        _ => Err(too_large()),
    }
}

pub fn set_kitty_png_size(mut image: KittyImage) -> Result<KittyImage, KittyPngError> {
    // f=100 is PNG per the protocol. Anything else (GIF, WebP, SVG, ...) would
    // be an unbounded decoder reachable from pty output (#880).
    if !ImageType::is_png(&image.data[..]) {
        return Err(KittyPngError::InvalidBytes(
            "Kitty f=100 payload is not a PNG.".to_string(),
        ));
    }

    // Header-only: the full decode happens later, off this thread, in the asset
    // cache, and the dimension limits are enforced here.
    let image_size = match ImageType::dimensions_from_bytes(&image.data[..]) {
        Ok(image_size) => image_size,
        Err(err) => return Err(KittyPngError::InvalidBytes(err.to_string())),
    };

    image.metadata.image_size = image_size.to_f32();

    Ok(image)
}

pub fn set_kitty_rgb_headers(mut image: KittyImage) -> Result<KittyImage, KittyRgbError> {
    let image_format = match image.metadata.pixel_data_format {
        KittyPixelDataFormat::Rgb24Bit => CustomImageFormat::Rgb,
        KittyPixelDataFormat::Rgba32Bit => CustomImageFormat::Rgba,
        KittyPixelDataFormat::Png => return Ok(image),
    };

    image.data = CustomImageHeader::prepend_custom_header(
        image.data,
        image.metadata.image_size.x() as u32,
        image.metadata.image_size.y() as u32,
        image_format,
    )?;

    Ok(image)
}

fn create_kitty_reply(image_id: u32, message: String) -> Vec<u8> {
    [
        C1::APC,
        b"G",
        format!("i={image_id};{message}").as_bytes(),
        C1::ST,
    ]
    .concat()
}

pub fn create_kitty_ok_reply(image_id: u32) -> Vec<u8> {
    create_kitty_reply(image_id, "OK".to_string())
}

pub fn create_kitty_error_reply(image_id: u32, err: KittyError) -> Vec<u8> {
    create_kitty_reply(image_id, format!("{err:?}"))
}

#[cfg(test)]
#[path = "kitty_tests.rs"]
mod tests;

#[cfg(all(test, feature = "local_fs"))]
mod local_fs_path_tests {
    use super::*;

    #[test]
    fn rejects_device_and_pseudo_file_paths() {
        for path in [
            "/dev/zero",
            "/dev/null",
            "/proc/self/environ",
            "/sys/kernel",
        ] {
            assert!(validate_kitty_file_path(path, false).is_err(), "{path}");
        }
    }

    #[test]
    fn rejects_directories_and_temp_without_marker() {
        let dir = env::temp_dir();
        assert!(validate_kitty_file_path(dir.to_str().unwrap(), false).is_err());

        let plain = dir.join(format!("kitty-824-plain-{}", std::process::id()));
        fs::write(&plain, b"x").unwrap();
        let plain_str = plain.to_str().unwrap();
        assert!(validate_kitty_file_path(plain_str, false).is_ok());
        assert!(
            validate_kitty_file_path(plain_str, true).is_err(),
            "t=t needs the tty-graphics-protocol marker"
        );
        fs::remove_file(&plain).unwrap();
    }

    fn marker_dir(name: &str) -> std::path::PathBuf {
        let dir = Path::new("/tmp").join(format!("{name}-{}", std::process::id()));
        fs::create_dir_all(dir.join("tty-graphics-protocol")).unwrap();
        dir
    }

    #[test]
    fn rejects_dotdot_traversal_out_of_marker_dir() {
        let dir = marker_dir("kitty-824-dotdot");
        let outside = dir.join("outside");
        fs::write(&outside, b"x").unwrap();
        let sneaky = format!("{}/tty-graphics-protocol/../outside", dir.to_str().unwrap());
        // Canonical path is `<dir>/outside`: in tmp but without the marker.
        assert!(validate_kitty_file_path(&sneaky, true).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_marker_named_symlink_pointing_outside_tmp() {
        let dir = marker_dir("kitty-824-symlink");
        let link = dir.join("tty-graphics-protocol-link");
        std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
        assert!(validate_kitty_file_path(link.to_str().unwrap(), true).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn temp_dir_match_is_component_wise() {
        assert!(!is_path_in_temp_dir(Path::new(
            "/tmpfoo/tty-graphics-protocol-x"
        )));
        assert!(!is_kitty_temp_path(Path::new(
            "/tmpfoo/tty-graphics-protocol-x"
        )));
        assert!(is_kitty_temp_path(Path::new(
            "/tmp/tty-graphics-protocol-x"
        )));
    }

    #[test]
    fn empty_tmpdir_does_not_match_everything() {
        // Single-threaded section: only this test touches TMPDIR.
        let saved = env::var_os("TMPDIR");
        unsafe { env::set_var("TMPDIR", "") };
        let matched = is_path_in_temp_dir(Path::new("/etc/passwd"));
        match saved {
            Some(value) => unsafe { env::set_var("TMPDIR", value) },
            None => unsafe { env::remove_var("TMPDIR") },
        }
        assert!(!matched);
    }
}
