//! BYOP one-shot 非流式补全适配层。
//!
//! 用于"主动式 AI"子链路(prompt suggestions / NLD predict / relevant files /
//! 会话标题生成等):需要发一次短请求拿到一段文本,**不需要 tool calling、
//! 不需要流式、不需要持久化到 task.messages**。
//!
//! 与 `chat_stream::generate_byop_output`(主对话流)的差别:
//! - 这里走 `Client::exec_chat`(非流式),一次性拿 `ChatResponse::first_text()`。
//! - 不接 `RequestParams` / `ResponseEvent` / `task_store`,纯字符串入字符串出。
//! - reasoning 默认禁(主动 AI 不应触发思考链 — 浪费 token + 慢),
//!   仅当 `OneshotOptions.allow_reasoning = true` 才按 capability gate 注入。
//!
//! 模型选择由调用方决定:`resolve_active_ai_oneshot()` 把 `active_ai_model`
//! (profile fallback 到 base_model)解码为 BYOP `OneshotConfig`,
//! 解码失败(没配 BYOP / 模型不在 BYOP 编码空间)→ 返回 `None`,
//! 调用方静默 no-op。

use anyhow::Context as _;
use futures::StreamExt;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent};
use warpui::{AppContext, EntityId, SingletonEntity as _};

use super::chat_stream;
use super::wire_log;
use crate::ai::llms::LLMPreferences;
use crate::settings::{AgentProviderApiType, ReasoningEffortSetting};

/// BYOP one-shot 请求所需的 provider/model 信息。
#[derive(Debug, Clone)]
pub struct OneshotConfig {
    pub base_url: String,
    pub api_key: String,
    pub model_id: String,
    pub api_type: AgentProviderApiType,
    pub reasoning_effort: ReasoningEffortSetting,
}

/// One-shot 调用的可选参数。
#[derive(Debug, Clone, Default)]
pub struct OneshotOptions {
    /// user message 字符截断上限(按 char,保护 CJK)。`None` = 默认 8000。
    pub max_chars: Option<usize>,
    /// 温度(genai `ChatOptions::temperature`),`None` = provider 默认。
    pub temperature: Option<f32>,
    /// 是否要求 JSON 输出(OpenAI 兼容 provider 走 response_format)。
    /// 注意:不支持的 adapter 会忽略此参数,系统提示词需要自身要求 JSON。
    pub response_format_json: bool,
    /// 是否允许触发 reasoning。默认 `false`(主动 AI 都是低延迟轻量调用)。
    pub allow_reasoning: bool,
    /// Wire-inspector category for this call. `None` -> `Kind::Oneshot`; title
    /// generation sets `Kind::TitleGen` so it is filterable in the inspector.
    pub wire_kind: Option<super::wire_log::Kind>,
}

const DEFAULT_MAX_CHARS: usize = 8000;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max).collect()
}

fn build_oneshot_request(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> (ChatRequest, ChatOptions) {
    let mut chat_opts = ChatOptions::default()
        .with_capture_content(true)
        .with_capture_usage(true);
    if let Some(t) = opts.temperature {
        chat_opts = chat_opts.with_temperature(t.into());
    }
    if opts.response_format_json {
        chat_opts = chat_opts.with_response_format(genai::chat::ChatResponseFormat::JsonMode);
    }
    if opts.allow_reasoning {
        if let Some(effort) = cfg.reasoning_effort.to_genai() {
            if super::reasoning::model_supports_reasoning(cfg.api_type, &cfg.model_id) {
                chat_opts = chat_opts.with_reasoning_effort(effort);
            }
        }
    }

    let max_chars = opts.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
    let user_truncated = truncate_chars(user, max_chars);

    let chat_req = ChatRequest::from_messages(vec![ChatMessage::user(user_truncated)])
        .with_system(system.to_owned());

    (chat_req, chat_opts)
}

/// 发送一次 BYOP 非流式 chat completion,返回模型 reply 的纯文本。
///
/// 错误吞由调用方决定 — 此处只 propagate `anyhow::Error`,不做日志。
pub async fn byop_oneshot_completion(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> anyhow::Result<String> {
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, cfg.api_key.clone());
    let (chat_req, chat_opts) = build_oneshot_request(cfg, system, user, opts);

    wire_capture_out(cfg, system, &chat_req, opts);

    let resp = client
        .exec_chat(&cfg.model_id, chat_req, Some(&chat_opts))
        .await
        .with_context(|| format!("byop oneshot exec_chat failed (model={})", cfg.model_id))?;

    let text = resp.first_text().unwrap_or("").to_owned();
    wire_capture_in(cfg, &text, opts);
    Ok(text)
}

/// Endpoint label matching the main path's `wire_adapter_label`.
fn wire_adapter(cfg: &OneshotConfig) -> String {
    format!("{:?} @ {}", cfg.api_type, cfg.base_url)
}

/// Outbound capture for a one-shot call. Gated on the inspector being armed only
/// (not a context window): these auxiliary calls — title generation, next-command,
/// etc. — have no usage meter, but the user still wants to see them go out.
fn wire_capture_out(
    cfg: &OneshotConfig,
    system: &str,
    chat_req: &ChatRequest,
    opts: &OneshotOptions,
) {
    if !wire_log::is_enabled() {
        return;
    }
    let kind = opts.wire_kind.unwrap_or(wire_log::Kind::Oneshot);
    let payload = match serde_json::to_string_pretty(&chat_req.messages) {
        Ok(s) => wire_log::Payload::Json(s),
        Err(e) => wire_log::Payload::Flagged(format!("serialize failed: {e}")),
    };
    wire_log::capture_out(
        kind,
        cfg.model_id.clone(),
        wire_adapter(cfg),
        payload,
        Some(wire_log::ContextSnapshot {
            system: (!system.is_empty()).then(|| system.to_owned()),
            ..Default::default()
        }),
    );
}

/// Inbound capture for a one-shot call: the model's reply text. No token usage —
/// one-shots do not report context occupancy.
fn wire_capture_in(cfg: &OneshotConfig, text: &str, opts: &OneshotOptions) {
    if !wire_log::is_enabled() {
        return;
    }
    let kind = opts.wire_kind.unwrap_or(wire_log::Kind::Oneshot);
    wire_log::capture_in(
        kind,
        cfg.model_id.clone(),
        wire_adapter(cfg),
        wire_log::Payload::Json(text.to_owned()),
        None,
    );
}

/// 发送一次 BYOP 流式 chat completion,聚合所有文本 chunk 后返回。
///
/// 给只接受 `stream=true` 的 OpenAI Responses 兼容代理使用。调用方仍然拿到完整
/// 字符串,因此可以继续复用 one-shot 的标题清洗 / JSON 解析逻辑。
pub async fn byop_oneshot_streaming_completion(
    cfg: &OneshotConfig,
    system: &str,
    user: &str,
    opts: &OneshotOptions,
) -> anyhow::Result<String> {
    let client = chat_stream::build_client(cfg.api_type, &cfg.base_url, cfg.api_key.clone());
    let (chat_req, chat_opts) = build_oneshot_request(cfg, system, user, opts);
    wire_capture_out(cfg, system, &chat_req, opts);
    let mut resp = client
        .exec_chat_stream(&cfg.model_id, chat_req, Some(&chat_opts))
        .await
        .with_context(|| {
            format!(
                "byop oneshot exec_chat_stream failed (model={})",
                cfg.model_id
            )
        })?
        .stream;

    let mut text = String::new();
    while let Some(event) = resp.next().await {
        match event.with_context(|| {
            format!(
                "byop oneshot exec_chat_stream event failed (model={})",
                cfg.model_id
            )
        })? {
            ChatStreamEvent::Chunk(chunk) => {
                text.push_str(&chunk.content);
            }
            ChatStreamEvent::Start
            | ChatStreamEvent::ReasoningChunk(_)
            | ChatStreamEvent::ThoughtSignatureChunk(_)
            | ChatStreamEvent::ToolCallChunk(_)
            | ChatStreamEvent::End(_) => {}
        }
    }

    wire_capture_in(cfg, &text, opts);
    Ok(text)
}

/// 解析当前 active profile 的 `active_ai_model`(fallback 到 `base_model`),
/// 若解码为合法 BYOP 编码 → 返回 `OneshotConfig`,否则 `None`(调用方静默 no-op)。
pub fn resolve_active_ai_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_ai_model(app, terminal_view_id)
        .id
        .clone();
    let (provider, api_key, model_id) = super::lookup_byop(app, &id)?;
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    Some(OneshotConfig {
        base_url: provider.base_url,
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
    })
}

/// 解析当前 active profile 的 `next_command_model`(fallback 到 `base_model`),
/// 若解码为合法 BYOP 编码 → 返回 `OneshotConfig`,否则 `None`。
pub fn resolve_next_command_oneshot(
    app: &AppContext,
    terminal_view_id: Option<EntityId>,
) -> Option<OneshotConfig> {
    let llm_prefs = LLMPreferences::as_ref(app);
    let id = llm_prefs
        .get_active_next_command_model(app, terminal_view_id)
        .id
        .clone();
    let (provider, api_key, model_id) = super::lookup_byop(app, &id)?;
    let reasoning_effort =
        llm_prefs.get_reasoning_effort(terminal_view_id, provider.api_type, &model_id);
    Some(OneshotConfig {
        base_url: provider.base_url,
        api_key,
        model_id,
        api_type: provider.api_type,
        reasoning_effort,
    })
}
