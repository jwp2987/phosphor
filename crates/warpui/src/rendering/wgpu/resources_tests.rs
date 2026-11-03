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
fn test_frame_latency_override_accepts_only_1_to_3() {
    assert_eq!(frame_latency_override(None), None);
    assert_eq!(frame_latency_override(Some("1")), Some(1));
    assert_eq!(frame_latency_override(Some("3")), Some(3));
    assert_eq!(frame_latency_override(Some("0")), None);
    assert_eq!(frame_latency_override(Some("9")), None);
    assert_eq!(frame_latency_override(Some("x")), None);
}
