use std::cell::RefCell;
use std::sync::Arc;
use warpui::{AppContext, Entity, ModelContext, ModelHandle};

use crate::{
    ai_assistant::execution_context::WarpAiExecutionContext,
    terminal::{
        model::session::SessionsEvent,
        model_events::{ModelEvent, ModelEventDispatcher},
        shell::ShellType,
        ShellLaunchData,
    },
};

use super::{Session, SessionType, Sessions};

pub struct ActiveSession {
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    sessions: ModelHandle<Sessions>,

    /// The current working directory of the terminal session.
    current_working_directory: Option<String>,

    /// The most recently successfully resolved execution environment (shell / os).
    ///
    /// `session()` returns `None` when the active session id is missing or the id
    /// doesn't resolve to a session (this can happen briefly during focus switches
    /// or session rebuilds). If we returned `None` directly in that case,
    /// `input_context_for_request` wouldn't push an `ExecutionEnvironment`, and the
    /// system prompt's <env> section would drop the Shell/Platform lines entirely —
    /// the model loses environment info, and a system section that changes turn to
    /// turn busts the prompt cache. We cache the last known value as a fallback.
    last_execution_environment: RefCell<Option<WarpAiExecutionContext>>,
}

impl ActiveSession {
    pub fn new(
        sessions: ModelHandle<Sessions>,
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model_event_dispatcher, move |me, event, ctx| {
            // Both the precmd path (`BlockMetadataReceived`) and the OSC 7 path
            // (`BlockWorkingDirectoryUpdated`) carry a fresh cwd for the active
            // block, so both must refresh the session's known directory.
            let block_metadata = match event {
                ModelEvent::BlockMetadataReceived(e) => Some(&e.block_metadata),
                ModelEvent::BlockWorkingDirectoryUpdated(e) => Some(&e.block_metadata),
                _ => None,
            };
            if let Some(block_metadata) = block_metadata {
                // Sticky update: block metadata without a cwd should not clear the
                // known directory. See the comment on
                // `BlocklistAIContextModel::update_directory_context` — if this got
                // cleared here, `list_skills` would silently degrade (see the
                // cwd-based skill-discovery call in `controller/input_context.rs`).
                let new_pwd = block_metadata
                    .current_working_directory()
                    .map(|cwd| cwd.to_owned());
                if new_pwd.is_some() && me.current_working_directory != new_pwd {
                    me.current_working_directory = new_pwd;
                    ctx.emit(ActiveSessionEvent::UpdatedPwd);
                }
            }
        });

        ctx.subscribe_to_model(&sessions, |me, event, ctx| {
            if let SessionsEvent::SessionBootstrapped(bootstrap_event) = event {
                if Some(bootstrap_event.session_id)
                    == me.model_event_dispatcher.as_ref(ctx).active_session_id()
                {
                    ctx.emit(ActiveSessionEvent::Bootstrapped);
                }
            }
        });

        Self {
            sessions,
            model_event_dispatcher,
            current_working_directory: None,
            last_execution_environment: RefCell::new(None),
        }
    }

    pub fn session(&self, app: &AppContext) -> Option<Arc<Session>> {
        self.model_event_dispatcher
            .as_ref(app)
            .active_session_id()
            .and_then(|session_id| self.sessions.as_ref(app).get(session_id))
    }

    pub fn session_type(&self, app: &AppContext) -> Option<SessionType> {
        self.session(app).map(|session| session.session_type())
    }

    pub fn shell_type(&self, app: &AppContext) -> Option<ShellType> {
        self.session(app)
            .as_ref()
            .map(|session| session.shell().shell_type())
    }

    pub fn shell_launch_data(&self, app: &AppContext) -> Option<ShellLaunchData> {
        self.session(app)
            .as_ref()
            .and_then(|session| session.launch_data().cloned())
    }

    pub fn current_working_directory(&self) -> Option<&String> {
        self.current_working_directory.as_ref()
    }

    /// Returns the current working directory as a [`LocalOrRemotePath`]. BYOP sessions are local,
    /// so this is always a `Local` path. Used by skill/slash-command surfaces that key off the
    /// working directory.
    pub fn current_working_directory_location(
        &self,
        _ctx: &AppContext,
    ) -> Option<warp_util::local_or_remote_path::LocalOrRemotePath> {
        self.current_working_directory
            .as_ref()
            .map(|cwd| warp_util::local_or_remote_path::LocalOrRemotePath::Local(cwd.into()))
    }

    /// Returns the `WarpAiExecutionContext` for the active session.
    ///
    /// Falls back to the last known value (see
    /// [`Self::last_execution_environment`]) when active-session resolution fails,
    /// avoiding losing the Shell/Platform lines from the system prompt's <env>
    /// section mid-conversation.
    pub fn ai_execution_environment(&self, app: &AppContext) -> Option<WarpAiExecutionContext> {
        if let Some(session) = self.session(app).as_ref() {
            let env = WarpAiExecutionContext::new(session);
            *self.last_execution_environment.borrow_mut() = Some(env.clone());
            return Some(env);
        }
        self.last_execution_environment.borrow().clone()
    }
}

pub enum ActiveSessionEvent {
    /// The active session's working directory changed.
    UpdatedPwd,
    /// The active session finished bootstrapping.
    Bootstrapped,
}

impl Entity for ActiveSession {
    type Event = ActiveSessionEvent;
}
