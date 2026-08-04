#[allow(deprecated)]
use cocoa::base::id;
use warpui::platform::mac::make_nsstring;
use warpui::AppContext;

use crate::channel::ChannelState;

#[cfg(feature = "release_bundle")]
use {
    service_impl::forward_uri_to_sole_running_instance,
    single_instance_manager::SingleInstanceManager, thiserror::Error, url::Url,
};

#[cfg(feature = "release_bundle")]
mod service_impl;
#[cfg(feature = "release_bundle")]
mod single_instance_manager;

unsafe extern "C" {
    /// ObjC function to create and register the NSServices provider for the
    /// application.
    fn warp_register_services_provider();
}

#[derive(Error, Debug)]
#[cfg(feature = "release_bundle")]
pub enum StartupArgsForwardingError {
    /// This instance was launched after an auto-update and should not forward
    /// arguments to the old (terminating) instance.
    #[error("should not forward arguments after an auto-update")]
    IgnoredAfterAutoUpdate,
    /// There's no instance of Zap already running.
    #[error("there is no other instance of Zap")]
    NoExistingInstance,
    #[error("failed to construct url")]
    CouldNotCreateUrl(#[from] url::ParseError),
    #[error("IPC Client failed to send message")]
    IpcError(#[from] ipc::ClientError),
}

/// Attempts to forward startup arguments to an existing instance of the application.
///
/// A GUI app launched by double-clicking the Dock/Finder icon is automatically deduplicated by
/// macOS's LaunchServices -- clicking an already-running app's icon just reactivates it, no
/// custom code needed. This function exists for the path LaunchServices *doesn't* cover: the
/// bundled `zap-oss` shell-integration script (`Contents/Resources/bin/zap-oss`) directly
/// `exec`s the main binary to support the `zap-oss <path>` terminal command, bypassing
/// LaunchServices entirely. Without this check, that command spawns a second, fully independent
/// GUI process -- its own window, its own Dock/App-Switcher entry -- instead of opening a new
/// window in the already-running instance, exactly like the Linux/Windows equivalents this
/// mirrors.
///
/// Returns Ok if an existing instance exists and was reachable.
#[cfg(feature = "release_bundle")]
pub fn pass_startup_args_to_existing_instance(
    args: &warp_cli::AppArgs,
) -> Result<(), StartupArgsForwardingError> {
    if args.finish_update {
        return Err(StartupArgsForwardingError::IgnoredAfterAutoUpdate);
    }
    if SingleInstanceManager::is_sole_running_instance() {
        return Err(StartupArgsForwardingError::NoExistingInstance);
    }

    warpui::r#async::block_on(async {
        if args.urls.is_empty() {
            // If there are no URLs on the command line, send one to open a new
            // window using the same current working directory as this process.
            let mut open_new_url = format!("{}://action/new_window", ChannelState::url_scheme());
            if let Ok(current_dir) = std::env::current_dir() {
                match current_dir.into_os_string().into_string() {
                    Ok(current_dir) => open_new_url.push_str(&format!("?path={}", current_dir)),
                    Err(os_string) => {
                        log::error!("Failed to convert OsString {os_string:?} to a string");
                    }
                }
            }

            let url = Url::parse(&open_new_url)?;
            forward_uri_to_sole_running_instance(vec![url]).await?
        } else {
            forward_uri_to_sole_running_instance(args.urls.clone()).await?
        }

        Ok(())
    })
}

/// Initializes application services.
pub fn init(_ctx: &mut AppContext) {
    unsafe {
        warp_register_services_provider();
    }

    #[cfg(feature = "release_bundle")]
    _ctx.add_singleton_model(SingleInstanceManager::new);
}

/// Returns an NSString containing the custom URL scheme that this build of the
/// application will respond to.
///
/// Called synchronously from the NSServices dispatch path in
/// `services.m::forFilesFromPasteboard:performAction:`, which wraps the body in
/// an `@autoreleasepool` block. That ambient pool owns the returned NSString.
#[allow(deprecated)]
#[unsafe(no_mangle)]
extern "C-unwind" fn warp_services_provider_custom_url_scheme() -> id {
    make_nsstring(ChannelState::url_scheme())
}
