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

    /// 最近一次成功解析出的执行环境(shell / os)。
    ///
    /// `session()` 会在 active session id 缺失或 id 查不到 session 时返回 `None`
    /// (焦点切换、session 重建期间都可能短暂发生)。此时若直接返回 `None`,
    /// `input_context_for_request` 就不会 push `ExecutionEnvironment`,
    /// system prompt 的 <env> 段会整段丢掉 Shell/Platform 两行 —— 模型丢失环境
    /// 信息,且 system 段逐轮变化击穿 prompt cache。缓存最后一次已知值兜底。
    last_execution_environment: RefCell<Option<WarpAiExecutionContext>>,
}

impl ActiveSession {
    pub fn new(
        sessions: ModelHandle<Sessions>,
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model_event_dispatcher, move |me, event, ctx| {
            if let ModelEvent::BlockMetadataReceived(block_metadata_received_event) = event {
                // 粘性更新:不带 cwd 的 block metadata 不应清空已知目录。
                // 详见 `BlocklistAIContextModel::update_directory_context` 的注释 ——
                // 此处若被置空,`list_skills` 会静默降级(见
                // `controller/input_context.rs` 中按 cwd 发现 skills 的调用)。
                let new_pwd = block_metadata_received_event
                    .block_metadata
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

    /// Returns the `WarpAiExecutionContext` for the active session.
    ///
    /// active session 解析失败时回退到最近一次已知值(见
    /// [`Self::last_execution_environment`]),避免 system prompt 的 <env> 段
    /// 在对话中途丢失 Shell/Platform 行。
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
