#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_family = "wasm")]
pub mod wasm;

pub fn init() {
    #[cfg(target_family = "wasm")]
    wasm::init();
}
