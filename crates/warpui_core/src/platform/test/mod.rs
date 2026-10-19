mod app;
mod delegate;

pub use app::App;
pub(crate) use delegate::WindowManager;
#[cfg(test)]
pub(crate) use delegate::set_forced_window_size;
pub use delegate::{
    AppDelegate, FontDB, IntegrationTestDelegate, RecordedSystemOpen,
    recorded_system_opens_matching,
};
