use super::*;

#[test]
fn windows_default_backends_leave_out_gl() {
    // #1629: the WGL instance thread's never-pumped top-level window hangs a layout switch.
    let backends = default_backends(true, false);
    assert!(!backends.contains(wgpu::Backends::GL));
    assert!(backends.contains(wgpu::Backends::DX12));
    assert!(backends.contains(wgpu::Backends::VULKAN));
}

#[test]
fn windows_gl_kill_switch_restores_all_backends() {
    assert_eq!(default_backends(true, true), wgpu::Backends::all());
}

#[test]
fn other_platforms_keep_all_backends() {
    assert_eq!(default_backends(false, false), wgpu::Backends::all());
    assert_eq!(default_backends(false, true), wgpu::Backends::all());
}
