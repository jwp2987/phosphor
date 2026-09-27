mod app;
mod delegate;

pub use app::App;
pub(crate) use delegate::WindowManager;
pub use delegate::{
    AppDelegate, FontDB, IntegrationTestDelegate, RecordedSystemOpen,
    recorded_system_opens_matching,
};
