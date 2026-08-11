use crate::ai::aws_credentials::AwsCredentialRefresher as _;
use crate::auth::AuthState;
use crate::auth::AuthStateProvider;
use crate::terminal::model::terminal_model::ExitReason;
use crate::terminal::shell::ShellName;
use crate::terminal::warpify::settings::WarpifySettings;
use crate::terminal::TerminalManager as TerminalManagerTrait;
use anyhow::Context as _;
use async_broadcast::InactiveReceiver;
use std::any::Any;
use std::sync::mpsc::{SendError, SyncSender};
use std::{collections::HashMap, ffi::OsString, path::PathBuf, sync::Arc, thread::JoinHandle};
use warp_core::SessionId;

use crate::terminal::available_shells::{AvailableShell, AvailableShells};
use crate::terminal::ShellLaunchData;
use crate::terminal::ShellLaunchState;

use parking_lot::{FairMutex, Mutex};
use pathfinder_geometry::vector::Vector2F;

use settings::Setting as _;
use warpui::r#async::executor::Background;
use warpui::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity, ViewHandle, WindowId};

use crate::ai::blocklist::{InputConfig, SerializedBlockListItem};
use crate::terminal::view::ConversationRestorationInNewPaneType;

use crate::banner::BannerState;
use crate::context_chips::current_prompt::CurrentPrompt;
use crate::context_chips::prompt_type::PromptType;
use crate::features::FeatureFlag;
use crate::pane_group::TerminalViewResources;
use crate::persistence::ModelEvent;

use crate::send_telemetry_on_executor;
use crate::server::telemetry::TelemetryEvent;
use crate::settings::DebugSettings;
use crate::settings::{PrivacySettings, SshSettings};

use crate::terminal::model::session::Sessions;

use crate::terminal::model_events::{ModelEventDispatcher, SshRemoteServerSupport};
use crate::terminal::safe_mode_settings::get_secret_obfuscation_mode;
use crate::terminal::session_settings::SessionSettings;
use crate::terminal::shared_session::SharedSessionStatus;
use crate::terminal::writeable_pty::pty_controller::{EventLoopSendError, EventLoopSender};
use crate::terminal::writeable_pty::terminal_manager_util::{
    init_pty_controller_model, init_remote_server_controller, wire_up_pty_controller_with_surface,
    wire_up_remote_server_controller_with_view,
};
use crate::terminal::writeable_pty::terminal_surface::{
    PtyIntentEvent, TerminalSurface, TerminalSurfaceInit, TerminalSurfaceResult,
};
use crate::terminal::writeable_pty::{self, Message};
use crate::terminal::{
    event_listener::ChannelEventListener,
    local_tty::{Pty, PtyOptions},
    TerminalModel,
};
use crate::terminal::{terminal_manager, TerminalView, PTY_READS_BROADCAST_CHANNEL_SIZE};

use super::mio_channel;
use super::recorder;
use super::shell::ShellStarter;
use super::{event_loop::EventLoop, shell::ShellStarterSource};

#[cfg(unix)]
use {
    super::terminal_attributes::TerminalAttributesPoller,
    crate::terminal::local_tty::terminal_attributes::Event as TerminalAttributesPollerEvent,
    crate::terminal::model::terminal_model::BlockIndex,
    crate::terminal::model_events::ModelEvent as TerminalModelEvent,
    nix::sys::termios::LocalFlags,
    std::{cell::RefCell, rc::Rc},
};

type PtyController = writeable_pty::PtyController<mio_channel::Sender<Message>>;
type RemoteServerController =
    writeable_pty::remote_server_controller::RemoteServerController<mio_channel::Sender<Message>>;

/// Owns a local terminal session: the terminal model, PTY event loop, PTY
/// controller, and a terminal surface `S`.
///
/// The manager talks to its frontend through the [`TerminalSurface`] contract
/// rather than a concrete view, so the same construction/PTY-interaction path
/// drives both the GUI `TerminalView` and the headless TUI surface. It also
/// holds onto any data that needs to live as long as the session does (e.g. the
/// event loop join handle).
pub struct TerminalManager<S> {
    event_loop_tx: Arc<Mutex<mio_channel::Sender<Message>>>,
    /// This is an `Option` so that we can take ownership of the inner
    /// `JoinHandle` in `TerminalManager::drop`.
    event_loop_handle: Option<JoinHandle<()>>,
    model: Arc<FairMutex<TerminalModel>>,
    view: ViewHandle<S>,

    /// The manager is responsible for managing the lifetime
    /// of the terminal attributes poller. None if the event loop has not yet started.
    #[cfg(unix)]
    #[allow(dead_code)]
    terminal_attributes_poller: Option<ModelHandle<TerminalAttributesPoller>>,

    /// The manager is responsible for managing the lifetime
    /// of the PTY controller.
    #[allow(dead_code)]
    pty_controller: ModelHandle<PtyController>,

    /// The manager is responsible for managing the lifetime of the remote server controller.
    #[allow(dead_code)]
    remote_server_controller: ModelHandle<RemoteServerController>,

    /// The process ID of the PTY. Purely used for integration tests. None if the PTY has not yet
    /// been started.
    #[cfg(feature = "integration_tests")]
    pid: Option<u32>,

    /// An inactive receiver for PTY reads that we can upgrade to an active
    /// receiver as needed. We prefer to not create active receivers eagerly
    /// to avoid unnecessary allocations of data coming from the PTY (high throughput).
    /// Note that we need to hold onto the inactive receiver so that the channel isn't closed prematurely.
    #[allow(dead_code)]
    inactive_pty_reads_rx: InactiveReceiver<Arc<Vec<u8>>>,
}

/// Handles created for a local terminal manager and its surface.
pub struct TerminalManagerInit<S> {
    pub manager: ModelHandle<Box<dyn TerminalManagerTrait>>,
    pub surface: ViewHandle<S>,
}

/// One-shot resources consumed when the shell is determined and the PTY starts.
struct ShellStartupResources {
    event_loop_rx: mio_channel::Receiver<Message>,
    channel_event_proxy: ChannelEventListener,
    #[cfg(unix)]
    model_events: ModelHandle<ModelEventDispatcher>,
}

/// Adapts a TUI-owned surface manager to Zap's type-erased manager contract.
///
/// The GUI boxes `TerminalManager<TerminalView>` directly (it implements the
/// object-safe trait, including the `TerminalView`-specific detach cleanup),
/// while TUI-owned surfaces are wrapped here so an arbitrary `S` can still be
/// stored as `Box<dyn TerminalManagerTrait>` without a view-specific impl.
struct TuiTerminalManager<S>(TerminalManager<S>);

impl<S: 'static> TerminalManagerTrait for TuiTerminalManager<S> {
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.0.model()
    }

    fn as_any(&self) -> &dyn Any {
        &self.0
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.0
    }
}

impl<S> Drop for TerminalManager<S> {
    fn drop(&mut self) {
        self.shutdown_event_loop();
    }
}

impl TerminalManager<TerminalView> {
    /// Creates a terminal manager model for the GUI `TerminalView` surface, and
    /// returns the view alongside the boxed, view-agnostic manager handle.
    ///
    /// Note that the order of operations in this constructor are important! See
    /// [`TerminalManager::create_model_with_manager`] for the shared, generic
    /// construction flow; this GUI entry point resolves the restored blocks,
    /// then supplies a surface-setup callback that builds the `TerminalView`
    /// (and its prompt models) plus the GUI-specific post-wiring.
    #[allow(clippy::too_many_arguments)]
    pub fn create_model(
        startup_directory: Option<PathBuf>,
        env_vars: HashMap<OsString, OsString>,
        resources: TerminalViewResources,
        restored_blocks: Option<&Vec<SerializedBlockListItem>>,
        conversation_restoration: Option<ConversationRestorationInNewPaneType>,
        user_default_shell_unsupported_banner_model_handle: ModelHandle<BannerState>,
        initial_size: Vector2F,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        window_id: WindowId,
        chosen_shell: Option<AvailableShell>,
        initial_input_config: Option<InputConfig>,
        ctx: &mut AppContext,
    ) -> (ViewHandle<TerminalView>, ModelHandle<Box<dyn TerminalManagerTrait>>) {
        // If we have explicit restored_blocks, prioritize those (these come from db on startup).
        // Otherwise if there's a conversation we're restoring, get blocks from those.
        let all_restored_blocks =
            restored_blocks
                .cloned()
                .or_else(|| match &conversation_restoration {
                    Some(ConversationRestorationInNewPaneType::Historical {
                        conversation, ..
                    })
                    | Some(ConversationRestorationInNewPaneType::Forked { conversation, .. }) => {
                        Some(conversation.to_serialized_blocklist_items())
                    }
                    _ => None,
                });

        // We need to append the session restoration separator to the block list if there are any
        // restored blocks (command blocks or AI conversations) to show. These predicates are
        // computed before `conversation_restoration` is moved into the surface callback.
        let has_restored_command_blocks = all_restored_blocks
            .as_ref()
            .is_some_and(|blocks| !blocks.is_empty());
        let has_conversation_restoration = matches!(
            &conversation_restoration,
            Some(
                ConversationRestorationInNewPaneType::Startup { .. }
                    | ConversationRestorationInNewPaneType::Historical { .. }
            )
        );
        let is_historical = matches!(
            &conversation_restoration,
            Some(ConversationRestorationInNewPaneType::Historical { .. })
        );
        let should_use_live_appearance = conversation_restoration
            .as_ref()
            .map(|restoration| restoration.should_use_live_appearance())
            .unwrap_or(false);
        let should_show_restoration_separator = (has_conversation_restoration
            || has_restored_command_blocks)
            && !should_use_live_appearance;

        // The surface callback captures the GUI-only inputs (resources, window,
        // prompt models, restoration, and initial input config) so the generic
        // manager core never needs to know about `TerminalView`.
        let surface_model_event_sender = model_event_sender.clone();
        let create_surface = move |surface_init: TerminalSurfaceInit, ctx: &mut AppContext| {
            let TerminalSurfaceInit {
                wakeups_rx,
                model_events,
                model,
                sessions,
                size_info,
                colors,
                inactive_pty_reads_rx,
            } = surface_init;

            let current_prompt = ctx.add_model(|ctx| {
                CurrentPrompt::new_with_model_events(sessions.clone(), Some(&model_events), ctx)
            });
            let prompt_type =
                ctx.add_model(|ctx| PromptType::new_dynamic(current_prompt.clone(), ctx));

            let cloned_model = model.clone();
            let view = ctx.add_typed_action_view(window_id, |ctx| {
                TerminalView::new(
                    resources,
                    wakeups_rx,
                    model_events.clone(),
                    cloned_model,
                    sessions.clone(),
                    size_info,
                    colors,
                    surface_model_event_sender,
                    prompt_type.clone(),
                    initial_input_config,
                    conversation_restoration,
                    Some(inactive_pty_reads_rx),
                    ctx,
                )
            });

            // GUI-specific post-wiring, run after the manager and PTY controller
            // exist (see `create_model_with_manager`).
            let post_wire = move |manager: &mut TerminalManager<TerminalView>,
                                  surface: &ViewHandle<TerminalView>,
                                  ctx: &mut AppContext| {
                if should_show_restoration_separator {
                    model
                        .lock()
                        .block_list_mut()
                        .append_session_restoration_separator_to_block_list(is_historical);
                }

                wire_up_remote_server_controller_with_view(
                    &manager.remote_server_controller,
                    surface,
                    ctx,
                );
            };

            TerminalSurfaceResult {
                surface: view,
                post_wire,
            }
        };

        let init = Self::create_model_with_manager(
            startup_directory,
            env_vars,
            all_restored_blocks.as_ref(),
            user_default_shell_unsupported_banner_model_handle,
            initial_size,
            model_event_sender,
            chosen_shell,
            SshRemoteServerSupport::Enabled,
            ctx,
            create_surface,
            |manager| Box::new(manager),
        );

        (init.surface, init.manager)
    }
}

impl<S> TerminalManager<S> {
    /// Creates a local terminal manager for a TUI-owned terminal surface.
    ///
    /// The caller supplies a surface-setup callback that builds the concrete
    /// surface (with no dependency on `TerminalView`) and any surface-specific
    /// post-wiring. Returns the boxed, view-agnostic manager alongside the
    /// surface handle.
    #[allow(clippy::too_many_arguments)]
    pub fn create_tui_model<PostWire>(
        startup_directory: Option<PathBuf>,
        env_vars: HashMap<OsString, OsString>,
        all_restored_blocks: Option<&Vec<SerializedBlockListItem>>,
        user_default_shell_unsupported_banner_model_handle: ModelHandle<BannerState>,
        initial_size: Vector2F,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        chosen_shell: Option<AvailableShell>,
        ctx: &mut AppContext,
        create_surface: impl FnOnce(TerminalSurfaceInit, &mut AppContext) -> TerminalSurfaceResult<S, PostWire>,
    ) -> TerminalManagerInit<S>
    where
        S: TerminalSurface,
        <S as Entity>::Event: PtyIntentEvent,
        PostWire: FnOnce(&mut Self, &ViewHandle<S>, &mut AppContext),
    {
        Self::create_model_with_manager(
            startup_directory,
            env_vars,
            all_restored_blocks,
            user_default_shell_unsupported_banner_model_handle,
            initial_size,
            model_event_sender,
            chosen_shell,
            SshRemoteServerSupport::Disabled,
            ctx,
            create_surface,
            |manager| Box::new(TuiTerminalManager(manager)),
        )
    }

    /// The canonical local terminal construction API, generic over the surface
    /// `S` and the type-erasure adapter `box_manager`.
    ///
    /// It owns the local terminal session construction order: creating the
    /// manager-owned channels/models/controllers, invoking one surface-setup
    /// callback that returns `(surface, post_wire)`, wiring the surface to the
    /// `PtyController` through [`wire_up_pty_controller_with_surface`], running
    /// `post_wire`, boxing the manager as a WarpUI model, scheduling shell
    /// determination, and returning `(manager, surface)`.
    #[allow(clippy::too_many_arguments)]
    fn create_model_with_manager<PostWire, BoxManager>(
        startup_directory: Option<PathBuf>,
        env_vars: HashMap<OsString, OsString>,
        all_restored_blocks: Option<&Vec<SerializedBlockListItem>>,
        user_default_shell_unsupported_banner_model_handle: ModelHandle<BannerState>,
        initial_size: Vector2F,
        model_event_sender: Option<SyncSender<ModelEvent>>,
        chosen_shell: Option<AvailableShell>,
        ssh_remote_server_support: SshRemoteServerSupport,
        ctx: &mut AppContext,
        create_surface: impl FnOnce(TerminalSurfaceInit, &mut AppContext) -> TerminalSurfaceResult<S, PostWire>,
        box_manager: BoxManager,
    ) -> TerminalManagerInit<S>
    where
        S: TerminalSurface,
        <S as Entity>::Event: PtyIntentEvent,
        PostWire: FnOnce(&mut Self, &ViewHandle<S>, &mut AppContext),
        BoxManager: FnOnce(Self) -> Box<dyn TerminalManagerTrait> + 'static,
    {
        // Create all the necessary channels we need for communication.
        let (wakeups_tx, wakeups_rx) = async_channel::unbounded();
        let (events_tx, events_rx) = async_channel::unbounded();
        let (executor_command_tx, executor_command_rx) = async_channel::unbounded();
        let (event_loop_tx, event_loop_rx) = mio_channel::channel();

        // Create the broadcast channel to receive data from the PTY, but deactivate it immediately.
        // We only want to create active receivers as necessary.
        let (pty_reads_tx, pty_reads_rx) =
            async_broadcast::broadcast(PTY_READS_BROADCAST_CHANNEL_SIZE);
        let inactive_pty_reads_rx = pty_reads_rx.deactivate();

        let channel_event_proxy = ChannelEventListener::new(wakeups_tx, events_tx, pty_reads_tx);

        // Initialize the sessions model.
        let sessions = ctx.add_model(|ctx| Sessions::new(executor_command_tx.clone(), ctx));

        let model_events = ctx.add_model(|ctx| {
            ModelEventDispatcher::new_with_ssh_remote_server_support(
                events_rx,
                sessions.clone(),
                ssh_remote_server_support,
                ctx,
            )
        });

        // Have ApiKeyManager subscribe to block completion events for AWS credential refresh
        ai::api_keys::ApiKeyManager::handle(ctx).update(ctx, |manager, ctx| {
            manager.register_model_event_dispatcher(&model_events, ctx);
        });

        let preferred_shell = chosen_shell.unwrap_or_else(|| {
            AvailableShells::handle(ctx)
                .read(ctx, |shells, ctx| shells.get_user_preferred_shell(ctx))
        });

        let wsl_name_or_shell_starter = ShellStarter::init(preferred_shell.clone());

        // Create the terminal model with all restored blocks
        let model = terminal_manager::create_terminal_model(
            startup_directory.clone(),
            all_restored_blocks,
            initial_size,
            channel_event_proxy.clone(),
            ShellLaunchState::DeterminingShell {
                available_shell: Some(preferred_shell),
                display_name: wsl_name_or_shell_starter
                    .as_ref()
                    .map(|wsl_name_or_shell_starter| wsl_name_or_shell_starter.name())
                    .unwrap_or(ShellName::LessDescriptive("Shell".to_owned())),
            },
            ctx,
        );
        let colors = model.colors();

        let model = Arc::new(FairMutex::new(model));

        // This is purely for measuring throughput on WarpDev.
        if FeatureFlag::RecordPtyThroughput.is_enabled() {
            Self::record_pty_throughput(inactive_pty_reads_rx.clone(), model.clone(), ctx);
        }

        // Initialize the PtyController.
        let pty_controller = init_pty_controller_model(
            event_loop_tx.clone(),
            executor_command_rx,
            model_events.clone(),
            sessions.clone(),
            model.clone(),
            ctx,
        );

        // Initialize the RemoteServerController.
        let remote_server_controller =
            init_remote_server_controller(&pty_controller, &model_events, ctx);

        // Build the concrete surface from the manager-created components.
        let size_info = model.lock().block_list().size().to_owned();
        let TerminalSurfaceResult { surface, post_wire } = create_surface(
            TerminalSurfaceInit {
                wakeups_rx,
                model_events: model_events.clone(),
                model: model.clone(),
                sessions: sessions.clone(),
                size_info,
                colors,
                inactive_pty_reads_rx: inactive_pty_reads_rx.clone(),
            },
            ctx,
        );

        wire_up_pty_controller_with_surface(
            &pty_controller,
            &surface,
            model.clone(),
            sessions,
            model_event_sender,
            ctx,
        );

        #[cfg(windows)]
        let event_loop_tx_clone = event_loop_tx.clone();

        // Create the terminal manager itself.
        let mut terminal_manager = Self {
            event_loop_tx: Arc::new(Mutex::new(event_loop_tx)),
            model,
            event_loop_handle: None,
            view: surface.clone(),
            #[cfg(unix)]
            terminal_attributes_poller: None,
            pty_controller,
            remote_server_controller,

            #[cfg(feature = "integration_tests")]
            pid: None,

            inactive_pty_reads_rx,
        };

        // Run surface-specific wiring after the manager exists, because this
        // step may need manager-owned controllers and retained handles.
        post_wire(&mut terminal_manager, &surface, ctx);

        let terminal_surface = surface.clone();
        let shell_startup_resources = ShellStartupResources {
            event_loop_rx,
            channel_event_proxy,
            #[cfg(unix)]
            model_events,
        };

        let terminal_manager_model = ctx.add_model(|ctx| {
            let terminal_manager = box_manager(terminal_manager);

            ctx.spawn(
                async move {
                    match wsl_name_or_shell_starter {
                        Some(starter_source) => starter_source.to_shell_starter_source().await,
                        None => None,
                    }
                },
                move |terminal_manager: &mut Box<dyn TerminalManagerTrait>,
                      shell_starter_source,
                      ctx| {
                    let Some(terminal_manager) =
                        TerminalManagerTrait::as_any_mut(terminal_manager.as_mut())
                            .downcast_mut::<Self>()
                    else {
                        return;
                    };

                    on_shell_determined(
                        terminal_manager,
                        startup_directory,
                        env_vars,
                        user_default_shell_unsupported_banner_model_handle,
                        #[cfg(windows)]
                        event_loop_tx_clone,
                        shell_startup_resources,
                        shell_starter_source,
                        ctx,
                    )
                },
            );

            terminal_manager
        });

        TerminalManagerInit {
            manager: terminal_manager_model,
            surface: terminal_surface,
        }
    }

    /// Returns the terminal model owned by this manager.
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.model.clone()
    }

    /// Sends a shutdown message to the PTY event loop and waits for it to
    /// process that event.
    fn shutdown_event_loop(&mut self) {
        let shutdown_res = self.event_loop_tx.lock().send(Message::Shutdown);
        // Happens normally if the event loop has already been terminated (so the channel is now gone).
        if let Err(e) = shutdown_res {
            log::info!("Failed to send Shutdown {e:?}");
        }

        match self.event_loop_handle.take() { Some(join_handle) => {
            if let Err(e) = join_handle.join() {
                log::error!("Failed to join event loop handle {e:?}");
            }
        } _ => {
            log::error!("No event loop handle to join when dropping terminal manager.")
        }}

        self.inactive_pty_reads_rx.close();
    }

    /// Sends bindkey to notify shell process to switch to PS1 logic for prompt
    /// with the combined prompt/command grid (we restore the saved PS1 value).
    pub fn send_switch_to_ps1_bindkey(&self, app_ctx: &mut AppContext) {
        self.pty_controller.update(app_ctx, |pty_controller, ctx| {
            pty_controller.send_switch_to_ps1_bindkey(ctx);
        });
    }

    /// Sends bindkey to notify shell process to switch to Zap prompt logic for prompt
    /// with the combined prompt/command grid (we unset the PS1, but save the value for potential
    /// future restoration).
    pub fn send_switch_to_warp_prompt_bindkey(&self, app_ctx: &mut AppContext) {
        self.pty_controller.update(app_ctx, |pty_controller, ctx| {
            pty_controller.send_switch_to_warp_prompt_bindkey(ctx);
        });
    }

    /// Records the PTY throughput by emitting a metric whenever the throughput
    /// is non-zero over some time interval.
    fn record_pty_throughput(
        pty_reads_rx: InactiveReceiver<Arc<Vec<u8>>>,
        model: Arc<FairMutex<TerminalModel>>,
        ctx: &mut AppContext,
    ) {
        if FeatureFlag::RecordPtyThroughput.is_enabled() {
            let auth_state = AuthStateProvider::as_ref(ctx).get();
            recorder::record_pty_throughput(
                pty_reads_rx.activate(),
                model,
                auth_state.clone(),
                ctx.background_executor().to_owned(),
            );
        }
    }

    fn enqueue_init_script(
        &self,
        shell_starter: &ShellStarter,
        session_id: SessionId,
    ) -> Result<(), SendError<Message>> {
        let shell_type = shell_starter.shell_type();
        if shell_type == crate::terminal::shell::ShellType::Zsh
            // For more on why this is necessary on Git Bash, see https://linear.app/warpdotdev/issue/CORE-3202.
            || shell_starter.is_msys2()
        {
            let init_shell_script = crate::terminal::bootstrap::init_shell_script_for_shell(
                shell_type,
                &crate::ASSETS,
                session_id,
            );
            let tx = self.event_loop_tx.lock();
            tx.send(Message::Input(init_shell_script.into_bytes().into()))?;
            tx.send(Message::Input(shell_type.execute_command_bytes().into()))
        } else {
            Ok(())
        }
    }

    fn create_pty(
        startup_directory: Option<PathBuf>,
        shell_starter: ShellStarter,
        env_vars: HashMap<OsString, OsString>,
        model: Arc<FairMutex<TerminalModel>>,
        #[cfg(windows)] event_loop_tx: mio_channel::Sender<Message>,
        ctx: &mut AppContext,
    ) -> anyhow::Result<Pty> {
        let is_shell_debug_mode_enabled = *DebugSettings::as_ref(ctx)
            .is_shell_debug_mode_enabled
            .value();
        let is_honor_ps1_enabled = *SessionSettings::as_ref(ctx).honor_ps1;
        let is_crash_reporting_enabled = PrivacySettings::as_ref(ctx).is_crash_reporting_enabled;

        // The TMUX SSH wrapper supercedes the original ControlMaster wrapper.
        let enable_ssh_wrapper = if FeatureFlag::SSHTmuxWrapper.is_enabled() {
            *WarpifySettings::as_ref(ctx)
                .enable_ssh_warpification
                .value()
                && !*WarpifySettings::as_ref(ctx).use_ssh_tmux_wrapper.value()
        } else {
            *SshSettings::as_ref(ctx).enable_legacy_ssh_wrapper.value()
        };

        let size: crate::terminal::SizeInfo = model.lock().block_list().size().to_owned();
        let options = PtyOptions {
            size,
            window_id: None,
            shell_starter,
            start_dir: startup_directory,
            env_vars,
            enable_ssh_wrapper,
            shell_debug_mode: is_shell_debug_mode_enabled,
            honor_ps1: is_honor_ps1_enabled,
            close_fds: true,
        };

        Pty::new(
            options,
            is_crash_reporting_enabled,
            #[cfg(windows)]
            event_loop_tx,
            ctx,
        )
    }

    /// Start's the PTY event loop, returning a sender for the event loop and the event loop's join handle.
    fn start_pty_event_loop(
        pty: Pty,
        rx: mio_channel::Receiver<Message>,
        model: Arc<FairMutex<TerminalModel>>,
        channel_event_proxy: ChannelEventListener,
    ) -> JoinHandle<()> {
        // Create the event loop and get a handle to the injector.
        let event_loop = EventLoop::new(model, channel_event_proxy, pty, rx);

        // Spawn the event loop on a separate thread to interact with the PTY and write the data back
        // to the terminal.
        event_loop.spawn()
    }

    #[cfg(feature = "integration_tests")]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

/// Callback invoked upon determining the shell to be spawned when starting the event loop.
#[allow(clippy::too_many_arguments)]
fn on_shell_determined<S: TerminalSurface>(
    manager: &mut TerminalManager<S>,
    startup_directory: Option<PathBuf>,
    env_vars: HashMap<OsString, OsString>,
    user_default_shell_unsupported_banner_model_handle: ModelHandle<BannerState>,
    #[cfg(windows)] event_loop_tx: mio_channel::Sender<Message>,
    shell_startup_resources: ShellStartupResources,
    shell_starter_source: Option<ShellStarterSource>,
    ctx: &mut ModelContext<Box<dyn TerminalManagerTrait>>,
) where
    <S as Entity>::Event: PtyIntentEvent,
{
    // This is executed as a callback and the window could be closed in the interim.
    if !ctx.is_window_open(manager.view.window_id(ctx)) {
        log::warn!("Window was closed before shell was determined, aborting shell startup.");
        return;
    }

    log::debug!("Using shell starter source {shell_starter_source:?}");
    let bg_executor = ctx.background_executor();
    let auth_state = AuthStateProvider::as_ref(ctx).get();

    let is_fallback_shell = matches!(
        shell_starter_source,
        Some(ShellStarterSource::Fallback { .. })
    );
    let shell_starter = shell_starter_source
        .map(|source| get_shell_starter_internal(source, bg_executor, auth_state));
    let shell_starter = match shell_starter {
        Some(shell_starter) => shell_starter,
        None => {
            log::error!("Could not compute fallback shell");
            manager.view.update(ctx, |surface, ctx| {
                surface.on_pty_spawn_failed(
                    anyhow::Error::msg("Could not find a fallback shell. If you have PowerShell or WSL installed, please file an issue."),
                    ctx,
                );
            });
            manager.model().lock().exit(ExitReason::ShellNotFound);
            return;
        }
    };

    // In WSL, default to the WSL home directory, not the native Windows home directory.
    let startup_directory = if let (ShellStarter::Wsl(wsl_shell_starter), None) =
        (&shell_starter, &startup_directory)
    {
        wsl_shell_starter.home_directory()
    } else {
        startup_directory
    };

    // Show a "shell unsupported" banner, if applicable.
    if is_fallback_shell
        && user_default_shell_unsupported_banner_model_handle.as_ref(ctx)
            == &BannerState::NotDismissed
    {
        user_default_shell_unsupported_banner_model_handle.update(ctx, |model, ctx| {
            *model = BannerState::Open;
            ctx.notify();
        })
    }

    manager
        .model()
        .lock()
        .set_login_shell_spawned(shell_starter.shell_type());

    let shell_launch_data = match &shell_starter {
        ShellStarter::Direct(shell_starter) => ShellLaunchData::Executable {
            executable_path: shell_starter.logical_shell_path().to_owned(),
            shell_type: shell_starter.shell_type(),
        },
        ShellStarter::DockerSandbox(docker_starter) => ShellLaunchData::Executable {
            executable_path: docker_starter.logical_shell_path().to_owned(),
            shell_type: docker_starter.shell_type(),
        },
        ShellStarter::Wsl(shell_starter) => ShellLaunchData::WSL {
            distro: shell_starter.distribution().to_owned(),
        },
        ShellStarter::MSYS2(shell_starter) => ShellLaunchData::MSYS2 {
            executable_path: shell_starter.logical_shell_path().to_owned(),
            shell_type: shell_starter.shell_type(),
        },
    };

    // This needs to be done before bootstrapping starts (i.e. before spawning the event loop below).
    manager
        .model()
        .lock()
        .set_pending_shell_launch_data(shell_launch_data.clone());

    // Register the session ID that was generated during shell starter construction.
    // For bash, fish, and PowerShell, the session ID is already baked into the command
    // args. For zsh and MSYS2, enqueue_init_script injects this same ID. This must happen
    // before the PTY is created below -- the shell must never be able to write anything
    // back before its session ID is registered, or a real DCS hook could arrive before
    // `is_registered_session` would recognize it.
    let generated_session_id = match &shell_starter {
        ShellStarter::Direct(starter) | ShellStarter::MSYS2(starter) => starter.session_id(),
        ShellStarter::DockerSandbox(starter) => starter.session_id(),
        ShellStarter::Wsl(starter) => starter.session_id(),
    };
    manager
        .model()
        .lock()
        .register_session_id(generated_session_id);

    // Enqueue the init shell script (for shells that need it), then create
    // the PTY and start its corresponding event loop.
    let ShellStartupResources {
        event_loop_rx,
        channel_event_proxy,
        #[cfg(unix)]
        model_events,
    } = shell_startup_resources;
    let model = manager.model();
    let pty = match manager
        .enqueue_init_script(&shell_starter, generated_session_id)
        .context("Failed to write shell init script to the pty")
        .and_then(|_| {
            TerminalManager::<S>::create_pty(
                startup_directory,
                shell_starter,
                env_vars,
                model.clone(),
                #[cfg(windows)]
                event_loop_tx,
                ctx,
            )
        }) {
        Ok(pty) => pty,
        Err(err) => {
            log::error!("Failed to spawn pty: {err:#}");
            manager.view.update(ctx, |surface, ctx| {
                surface.on_pty_spawn_failed(err, ctx);
            });
            manager.model().lock().exit(ExitReason::PtySpawnFailed);
            return;
        }
    };

    #[cfg(feature = "integration_tests")]
    let pid = pty.get_pid();
    #[cfg(unix)]
    let fd = pty.get_fd();

    // Create the channel above and pass the receving side to the event loop.
    let event_loop_handle = TerminalManager::<S>::start_pty_event_loop(
        pty,
        event_loop_rx,
        model.clone(),
        channel_event_proxy,
    );

    manager.event_loop_handle = Some(event_loop_handle);
    #[cfg(feature = "integration_tests")]
    {
        manager.pid = Some(pid);
    }

    manager.view.update(ctx, |surface, ctx| {
        surface.on_shell_determined(ctx);
        surface.on_active_shell_launch_data_updated(Some(shell_launch_data), ctx);
    });

    // Initialize the terminal attributes poller.
    // TODO(CORE-2297): Implement TerminalPoller on Windows.
    #[cfg(unix)]
    {
        let terminal_attributes_poller = ctx.add_model(|_| TerminalAttributesPoller::new(fd));
        wire_up_terminal_attribute_poller_with_surface(
            &terminal_attributes_poller,
            &manager.view,
            &model_events,
            model.clone(),
            ctx,
        );

        manager.terminal_attributes_poller = Some(terminal_attributes_poller);
    }
}

/// Wires the Unix terminal-attributes poller to a terminal surface.
///
/// The poller is driven off `ModelEventDispatcher` block start/complete events
/// rather than surface-specific view events, and termios results are routed back
/// through the [`TerminalSurface`] Unix hooks so the manager stays surface-agnostic.
///
/// NOTE: we cannot simply use the strong references (the handle arguments to this
/// wire_up fn) in the subscription callbacks because that will create a reference
/// cycle. Instead, we use weak handles and upgrade them lazily.
#[cfg(unix)]
fn wire_up_terminal_attribute_poller_with_surface<S: TerminalSurface>(
    terminal_attributes_poller: &ModelHandle<TerminalAttributesPoller>,
    surface: &ViewHandle<S>,
    model_events: &ModelHandle<ModelEventDispatcher>,
    model: Arc<FairMutex<TerminalModel>>,
    ctx: &mut ModelContext<Box<dyn TerminalManagerTrait>>,
) where
    <S as Entity>::Event: PtyIntentEvent,
{
    let poller_weak_handle = terminal_attributes_poller.downgrade();
    let poller_weak_handle_for_termios = poller_weak_handle.clone();
    let surface_weak_handle = surface.downgrade();
    let surface_weak_handle_for_termios = surface_weak_handle.clone();

    // Tracks the index of the started block across the model-event <-> poller interactions.
    let block_index: Rc<RefCell<Option<BlockIndex>>> = Rc::new(RefCell::new(None));
    let block_index_for_termios = block_index.clone();

    // On block start, record the active block index and ask the surface whether
    // password-prompt polling is useful before starting the poller. On block
    // completion, stop polling and let the surface react to the completed block.
    ctx.subscribe_to_model(model_events, move |_manager, event, ctx| {
        let Some(poller) = poller_weak_handle.upgrade(ctx) else {
            return;
        };

        match event {
            TerminalModelEvent::AfterBlockStarted {
                command,
                is_for_in_band_command: false,
                ..
            } => {
                let should_poll = surface_weak_handle.upgrade(ctx).is_some_and(|surface| {
                    surface.read(ctx, |surface, ctx| {
                        surface.should_start_password_prompt_polling(command, ctx)
                    })
                });
                if should_poll {
                    *block_index.borrow_mut() =
                        Some(model.lock().block_list().active_block_index());
                    poller.update(ctx, |poller, ctx| {
                        poller.start_polling(ctx);
                    });
                } else {
                    *block_index.borrow_mut() = None;
                }
            }
            TerminalModelEvent::AfterBlockCompleted(completed) => {
                if let Some(surface) = surface_weak_handle.upgrade(ctx) {
                    let should_stop = surface.read(ctx, |surface, _ctx| {
                        surface.should_stop_password_prompt_polling(completed)
                    });
                    if should_stop {
                        poller.update(ctx, |poller, _ctx| {
                            poller.stop_polling();
                        });
                        surface.update(ctx, |surface, ctx| {
                            surface.on_polled_block_completed(completed, ctx);
                        });
                    }
                }
            }
            _ => {}
        }
    });

    // When the poller detects termios consistent with a password prompt, route the
    // result through the surface and stop after the first detection to preserve
    // today's one-notification-per-command behavior.
    ctx.subscribe_to_model(terminal_attributes_poller, move |_manager, event, ctx| {
        let TerminalAttributesPollerEvent::TermiosQueryFinished { termios } = event;

        // A PTY likely has a password prompt if ECHO is disabled but ICANON is
        // still enabled. Apps like neovim disable both in raw mode, so requiring
        // ICANON avoids false positives.
        let might_be_password_prompt = !termios.local_flags.contains(LocalFlags::ECHO)
            && termios.local_flags.contains(LocalFlags::ICANON);
        if !might_be_password_prompt {
            return;
        }

        let Some(surface) = surface_weak_handle_for_termios.upgrade(ctx) else {
            return;
        };
        let block_index = block_index_for_termios.borrow_mut().take();
        surface.update(ctx, |surface, ctx| {
            surface.on_possible_password_prompt(block_index, ctx);
        });

        if let Some(poller) = poller_weak_handle_for_termios.upgrade(ctx) {
            poller.update(ctx, |poller, _ctx| {
                poller.stop_polling();
            });
        }
    });
}

/// Contains necessary logic for stopping the current shared session.
fn cleanup_shared_session(
    terminal_view: &ViewHandle<TerminalView>,
    model: Arc<FairMutex<TerminalModel>>,
    ctx: &mut AppContext,
) {
    let mut model_lock = model.lock();
    if !model_lock.shared_session_status().is_sharer() {
        log::warn!("Attempted to stop sharing current session that is not being shared");
        return;
    }

    // Change the status of the session to unshared.
    model_lock.set_shared_session_status(SharedSessionStatus::NotShared);
    model_lock.set_obfuscate_secrets(get_secret_obfuscation_mode(ctx));
    model_lock.clear_ordered_terminal_events_for_shared_session_tx();

    // Drop the lock so that it can be taken by the other entities that
    // need to do cleanup.
    drop(model_lock);

    terminal_view.update(ctx, |view, ctx| {
        view.on_session_share_ended(ctx);
    });
}

pub fn get_shell_starter(
    chosen_shell: Option<AvailableShell>,
    auth_state: &AuthState,
    ctx: &mut AppContext,
) -> Option<ShellStarter> {
    let preferred_shell = chosen_shell.unwrap_or_else(|| {
        AvailableShells::handle(ctx).read(ctx, |shells, ctx| shells.get_user_preferred_shell(ctx))
    });
    let shell_starter_or_wsl_name = ShellStarter::init(preferred_shell);

    // TODO(alokedesai): Further refactor this function to make it clear that it's expensive.
    shell_starter_or_wsl_name
        .and_then(|starter| {
            warpui::r#async::block_on(async { starter.to_shell_starter_source().await })
        })
        .map(|starter_source| {
            get_shell_starter_internal(
                starter_source,
                ctx.background_executor().clone(),
                auth_state,
            )
        })
}

fn get_shell_starter_internal(
    shell_starter_source: ShellStarterSource,
    background_executor: Arc<Background>,
    auth_state: &AuthState,
) -> ShellStarter {
    match shell_starter_source {
        ShellStarterSource::Override(shell_starter) => shell_starter,
        ShellStarterSource::Environment(starter) | ShellStarterSource::UserDefault(starter) => {
            ShellStarter::Direct(starter)
        }
        ShellStarterSource::Fallback {
            unsupported_shell,
            starter,
        } => {
            if let Some(unsupported_shell) = unsupported_shell {
                send_telemetry_on_executor!(
                    auth_state,
                    TelemetryEvent::UnsupportedShell {
                        shell: unsupported_shell
                    },
                    background_executor
                );
            }

            ShellStarter::Direct(starter)
        }
    }
}

/// Send a Shutdown event to each PTY's event loop and waits for the
/// event loop to terminate.
/// This is needed on Windows to ensure all OpenConsole processes are
/// cleaned up before the main thread exits.
#[cfg(windows)]
pub fn shutdown_all_pty_event_loops(ctx: &mut AppContext) {
    let terminal_managers: Vec<ModelHandle<Box<dyn TerminalManagerTrait>>> = ctx.models_of_type();
    terminal_managers.into_iter().for_each(|terminal_manager| {
        terminal_manager.update(ctx, |terminal_manager, _ctx| {
            if let Some(manager) = terminal_manager
                .as_any_mut()
                .downcast_mut::<TerminalManager<TerminalView>>()
            {
                manager.shutdown_event_loop();
            }
        })
    })
}

impl TerminalManagerTrait for TerminalManager<TerminalView> {
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.model.clone()
    }

    fn on_view_detached(
        &self,
        // The detach type is intentionally ignored: a sharer always stops sharing immediately,
        // even on a reversible `HiddenForClose` detach. This is desirable for security — a sharer
        // should not continue accepting commands from viewers while the session is not visible.
        _detach_type: crate::pane_group::pane::DetachType,
        app: &mut AppContext,
    ) {
        let shared_session_status = self.model.lock().shared_session_status().clone();
        if shared_session_status.is_sharer() {
            cleanup_shared_session(&self.view, self.model.clone(), app);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl EventLoopSender for mio_channel::Sender<Message> {
    fn send(&self, message: Message) -> Result<(), EventLoopSendError> {
        self.send(message).map_err(|error| match error {
            SendError(_) => EventLoopSendError::Disconnected,
        })
    }
}

impl TerminalSurfaceInit {
    /// Creates mock terminal surface inputs without spawning a PTY.
    ///
    /// Ported from Warp OSS (`terminal_manager.rs`) for the `warp_tui` test suite;
    /// gated on `test-util` so it never enters the shipping build.
    #[cfg(any(test, feature = "test-util"))]
    pub fn new_for_test(ctx: &mut AppContext) -> Self {
        let (_wakeups_tx, wakeups_rx) = async_channel::unbounded();
        let (_events_tx, events_rx) = async_channel::unbounded();
        let (pty_reads_tx, pty_reads_rx) =
            async_broadcast::broadcast(PTY_READS_BROADCAST_CHANNEL_SIZE);
        drop(pty_reads_tx);
        let sessions = ctx.add_model(|_| Sessions::new_for_test());
        let model_events =
            ctx.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
        let model = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
        let colors = model.lock().colors();
        let size_info = model.lock().block_list().size().to_owned();
        Self {
            wakeups_rx,
            model_events,
            model,
            sessions,
            size_info,
            colors,
            inactive_pty_reads_rx: pty_reads_rx.deactivate(),
        }
    }
}
