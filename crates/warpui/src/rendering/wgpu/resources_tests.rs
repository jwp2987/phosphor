use super::*;

#[test]
fn test_is_unsupported_llvmpipe_adapter() {
    let supported_adapter_info = wgpu::AdapterInfo {
        name: "llvmpipe (LLVM 17.0.6, 256 bits)".to_owned(),
        // not used
        vendor: 0,
        // not used
        device: 0,
        device_type: wgpu::DeviceType::Cpu,
        driver: "llvmpipe".to_owned(),
        driver_info: "Mesa 24.0.2-arch1.2 (LLVM 17.0.6)".to_owned(),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    };
    assert!(!is_older_lavapipe_adapter(&supported_adapter_info));

    let unsupported_adapter_info = wgpu::AdapterInfo {
        name: "llvmpipe (LLVM 17.0.6, 256 bits)".to_owned(),
        // not used
        vendor: 0,
        // not used
        device: 0,
        device_type: wgpu::DeviceType::Cpu,
        driver: "llvmpipe".to_owned(),
        driver_info: "Mesa 23.2.1-1ubuntu3.1~22.04.2 (LLVM 15.0.7)".to_owned(),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    };

    assert!(is_older_lavapipe_adapter(&unsupported_adapter_info));
}

#[test]
fn test_is_unsupported_intel_uhd_adapter() {
    assert!(is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        driver_info: String::from("Mesa 21.2.6"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(!is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // Version is recent enough
        driver_info: String::from("Mesa 23.2.6"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(!is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // Info string is messed up
        driver_info: String::from("Mssa 21.2.6"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // Additional info should be ignored
        driver_info: String::from("Mesa 21.2.6 foo bar"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(!is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // No version number
        driver_info: String::from("Mesa"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) HD Graphics 620 (KBL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // Nonsense version string
        driver_info: String::from("Mesa wtfis&this"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    // See https://github.com/warpdotdev/warp/issues/14577.
    assert!(is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) Xe Graphics (TGL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        driver_info: String::from("Mesa 21.2.6"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
    assert!(!is_older_vulkan_intel_uhd_adapter(&wgpu::AdapterInfo {
        name: String::from("Intel(R) Xe Graphics (TGL GT2)"),
        vendor: 0,
        device: 0,
        device_type: wgpu::DeviceType::IntegratedGpu,
        driver: String::from("Intel open-source Mesa driver"),
        // Version is recent enough
        driver_info: String::from("Mesa 24.0.2"),
        backend: wgpu::Backend::Vulkan,
        device_pci_bus_id: "01:00.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }));
}

#[test]
fn test_desired_frame_latency_only_overrides_dx12() {
    assert_eq!(desired_frame_latency(Backend::Dx12), Some(1));
    for backend in [
        Backend::Vulkan,
        Backend::Gl,
        Backend::Metal,
        Backend::BrowserWebGpu,
        Backend::Noop,
    ] {
        assert_eq!(desired_frame_latency(backend), None, "{backend:?}");
    }
}

#[test]
fn test_resolve_auto_no_vsync_prefers_immediate_then_mailbox_then_fifo() {
    use PresentMode::*;
    assert_eq!(
        resolve_auto_no_vsync(&[Fifo, Mailbox, Immediate]),
        Immediate
    );
    assert_eq!(resolve_auto_no_vsync(&[Fifo, Mailbox]), Mailbox);
    assert_eq!(resolve_auto_no_vsync(&[Fifo]), Fifo);
    assert_eq!(resolve_auto_no_vsync(&[]), Fifo);
}

#[test]
fn block_on_bounded_returns_ready_value() {
    assert_eq!(
        block_on_bounded(async { 7 }, Duration::from_millis(50)),
        Some(7)
    );
}

#[test]
fn block_on_bounded_gives_up_on_pending_future() {
    let start = std::time::Instant::now();
    let result = block_on_bounded(std::future::pending::<()>(), Duration::from_millis(50));
    assert!(result.is_none());
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn test_present_mode_override_parses_and_checks_support() {
    use wgpu::PresentMode::*;
    let all = [Fifo, Mailbox, Immediate];
    assert_eq!(present_mode_override(None, &all), None);
    assert_eq!(present_mode_override(Some("Mailbox"), &all), Some(Mailbox));
    assert_eq!(present_mode_override(Some(" fifo "), &all), Some(Fifo));
    assert_eq!(
        present_mode_override(Some("auto_vsync"), &all),
        Some(AutoVsync)
    );
    assert_eq!(present_mode_override(Some("bogus"), &all), None);
    assert_eq!(present_mode_override(Some("immediate"), &[Fifo]), None);
}

#[test]
fn present_never_waits_only_for_immediate_and_mailbox() {
    use wgpu::PresentMode::*;
    // #1843: a key's frame is drawn ahead of the compositor's frame callback only when the
    // present cannot park the main thread.
    let all = [Mailbox, Fifo, Immediate];
    assert!(present_never_waits(Immediate, &all));
    assert!(present_never_waits(Mailbox, &all));
    assert!(!present_never_waits(Fifo, &all));
    assert!(!present_never_waits(FifoRelaxed, &all));
    assert!(!present_never_waits(AutoVsync, &all));
    // AutoNoVsync resolves the way wgpu does: Immediate, else Mailbox, else Fifo.
    assert!(present_never_waits(AutoNoVsync, &all));
    assert!(present_never_waits(AutoNoVsync, &[Fifo, Mailbox]));
    assert!(!present_never_waits(AutoNoVsync, &[Fifo]));
    assert!(!present_never_waits(AutoNoVsync, &[]));
}

#[test]
fn test_frame_latency_override_accepts_only_1_to_3() {
    assert_eq!(frame_latency_override(None), None);
    assert_eq!(frame_latency_override(Some("1")), Some(1));
    assert_eq!(frame_latency_override(Some("3")), Some(3));
    assert_eq!(frame_latency_override(Some("0")), None);
    assert_eq!(frame_latency_override(Some("9")), None);
    assert_eq!(frame_latency_override(Some("x")), None);
}

fn summary_test_adapter(
    name: &str,
    device_type: wgpu::DeviceType,
    backend: wgpu::Backend,
) -> wgpu::AdapterInfo {
    wgpu::AdapterInfo {
        name: name.to_owned(),
        vendor: 0,
        device: 0,
        device_type,
        driver: String::new(),
        driver_info: String::new(),
        backend,
        device_pci_bus_id: "00:02.0".to_owned(),
        subgroup_min_size: wgpu::MINIMUM_SUBGROUP_MIN_SIZE,
        subgroup_max_size: wgpu::MAXIMUM_SUBGROUP_MAX_SIZE,
        transient_saves_memory: false,
    }
}

fn summary_test_config(present_mode: PresentMode) -> SurfaceConfiguration {
    SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: SURFACE_SIZE_FOR_TESTING,
        height: SURFACE_SIZE_FOR_TESTING,
        present_mode,
        desired_maximum_frame_latency: 2,
        alpha_mode: CompositeAlphaMode::PostMultiplied,
        view_formats: vec![],
    }
}

/// The app test `gpu_real_render` (script/realapp/appci/cases_r15_w5c.py, `SURFACE_RE`) reads this
/// line to learn which adapter a window draws on and whether it is a software rasteriser. Its
/// wording is that test's contract: change both together.
#[test]
fn surface_config_summary_names_the_adapter_and_flags_a_software_renderer() {
    let gpu = summary_test_adapter(
        "Intel(R) UHD Graphics 630 (CFL GT2)",
        wgpu::DeviceType::IntegratedGpu,
        wgpu::Backend::Vulkan,
    );
    assert_eq!(
        surface_config_summary(
            &gpu,
            &summary_test_config(PresentMode::Fifo),
            &[PresentMode::Fifo]
        ),
        "Surface configured: backend=Vulkan adapter=\"Intel(R) UHD Graphics 630 (CFL GT2)\" \
         present_mode=Fifo frame_latency=2 alpha_mode=PostMultiplied software_renderer=false"
    );

    let software = summary_test_adapter(
        "llvmpipe (LLVM 20.1.2, 256 bits)",
        wgpu::DeviceType::Cpu,
        wgpu::Backend::Gl,
    );
    assert_eq!(
        surface_config_summary(
            &software,
            &summary_test_config(PresentMode::AutoNoVsync),
            &[PresentMode::Fifo, PresentMode::Mailbox]
        ),
        "Surface configured: backend=Gl adapter=\"llvmpipe (LLVM 20.1.2, 256 bits)\" \
         present_mode=Mailbox (via AutoNoVsync, supported: [Fifo, Mailbox]) frame_latency=2 \
         alpha_mode=PostMultiplied software_renderer=true"
    );
}
