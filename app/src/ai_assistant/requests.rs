use chrono::{OutOfRangeError, Utc};
use futures::stream::AbortHandle;

use warp_core::user_preferences::GetUserPreferences as _;
use warpui::{AppContext, Entity, ModelContext};

use crate::{
    ai::{RequestLimitInfo, RequestUsageInfo},
    ai_assistant::utils::{AssistantTranscriptPart, TranscriptPartSubType},
    send_telemetry_from_ctx,
    server::telemetry::{TelemetryEvent, WarpAIRequestResult},
};

use super::{
    execution_context::WarpAiExecutionContext,
    utils::{markdown_segments_from_text, FormattedTranscriptMessage, TranscriptPart},
};
use anyhow::Result;

/// The key for the corresponding entry in UserDefaults.
/// Not wiring through Settings for now since this data is only needed by the panel view.
pub const REQUEST_LIMIT_INFO_CACHE_KEY: &str = "AIAssistantRequestLimitInfo";

/// Tracks the current request status for making Zap AI requests against server.
pub enum RequestStatus {
    /// There isn't a request in flight right now.
    NotInFlight,

    /// There's currently a request in flight.
    InFlight {
        /// The request itself (i.e. the prompt).
        request: FormattedTranscriptMessage,
        /// A handle to abort the request if desired.
        abort_handle: AbortHandle,
    },
}

fn cache_request_limit_info(request_limit_info: RequestLimitInfo, app_mut: &mut AppContext) {
    if let Ok(serialized) = serde_json::to_string(&request_limit_info) {
        let _ = app_mut
            .private_user_preferences()
            .write_value(REQUEST_LIMIT_INFO_CACHE_KEY, serialized);
    }
}

fn get_cached_request_limit_info(app_mut: &mut AppContext) -> Option<RequestLimitInfo> {
    app_mut
        .private_user_preferences()
        .read_value(REQUEST_LIMIT_INFO_CACHE_KEY)
        .unwrap_or_default()
        .and_then(|serialized| serde_json::from_str(serialized.as_str()).ok())
}

pub struct Requests {
    request_status: RequestStatus,
    request_limit_info: RequestLimitInfo,

    /// The currently displayed transcript.
    current_transcript: Vec<TranscriptPart>,

    /// Has the server summarized the current transcript because it's running long?
    current_transcript_summarized: bool,

    /// When a user Restarts their transcript, we still remember
    /// the previous transcript parts for things like suggestions.
    /// This list is mutually exclusive from current_transcript.  
    old_transcript_parts: Vec<TranscriptPart>,
}

impl Entity for Requests {
    type Event = Event;
}

pub enum Event {
    RequestFinished { succeeded: bool },
}

/// The delay after which a countdown of `remaining` renders different text.  Matches the
/// flooring in `serialized_time_until_refresh`; one second of margin lands the wake just past
/// the boundary.
fn countdown_text_change_delay(remaining: std::time::Duration) -> std::time::Duration {
    let secs = remaining.as_secs();
    let unit = if secs >= 24 * 3600 {
        24 * 3600
    } else if secs >= 3600 {
        3600
    } else {
        60
    };
    let boundary = (secs / unit) * unit;
    std::time::Duration::from_secs(secs - boundary + 1)
}

#[cfg(test)]
mod countdown_tests {
    use super::countdown_text_change_delay as delay;
    use std::time::Duration;

    #[test]
    fn wakes_at_the_next_unit_boundary_not_every_minute() {
        // 365 days: text is "365 days"; it next changes when 364d23h... i.e. within a day.
        assert_eq!(
            delay(Duration::from_secs(365 * 86400 + 5000)),
            Duration::from_secs(5001)
        );
        // 3 hours 10 min: next change in 10 min (+1 s), not 60 s.
        assert_eq!(
            delay(Duration::from_secs(3 * 3600 + 600)),
            Duration::from_secs(601)
        );
        // 5 minutes 20 s: the minute label drops after 20 s.
        assert_eq!(
            delay(Duration::from_secs(5 * 60 + 20)),
            Duration::from_secs(21)
        );
        // Under a minute: label reads "0 minutes" until the countdown ends.
        assert_eq!(delay(Duration::from_secs(30)), Duration::from_secs(31));
    }
}

/// Private interface.
impl Requests {
    fn remaining_time_to_refresh_std(&self) -> Result<std::time::Duration, OutOfRangeError> {
        self.request_limit_info
            .next_refresh_time
            .utc()
            .signed_duration_since(Utc::now())
            .to_std()
    }
}

/// Public interface.
impl Requests {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Check if the user has cached request limit info from before.
        // If not, let's just make an assumption about the server's default request limit
        // and fetch the true request limit later.
        let cached_request_limit_info = get_cached_request_limit_info(ctx);
        let request_limit_info = cached_request_limit_info.unwrap_or_default();

        let requests = Self {
            current_transcript: Vec::new(),
            current_transcript_summarized: false,
            old_transcript_parts: Vec::new(),
            request_status: RequestStatus::NotInFlight,
            request_limit_info,
        };

        // Zap: with no Zap Inc cloud backend, the initial request_limit_info
        // is no longer fetched from the server; using the cached / default
        // value above is sufficient.
        let _ = cached_request_limit_info;
        requests
    }

    pub fn update_ai_execution_context(
        &mut self,
        _ai_execution_context: Option<WarpAiExecutionContext>,
    ) {
    }

    pub fn update_request_limit_info(
        &mut self,
        result: Result<RequestUsageInfo>,
        ctx: &mut ModelContext<Self>,
    ) {
        match result {
            Ok(usage_info) => {
                self.request_limit_info = usage_info.request_limit_info;
                ctx.notify();
                cache_request_limit_info(usage_info.request_limit_info, ctx);
            }
            Err(e) => {
                log::warn!("Failed to retrieve initial request limit info: {e:#}");
            }
        }
    }

    /// Starts a Zap AI request against the server with the given request prompt.
    pub fn issue_request(&mut self, request: String, ctx: &mut ModelContext<Self>) {
        let raw_request = request.trim();
        let transcript_part_index = self.current_transcript.len();

        let request_in_markdown = markdown_segments_from_text(
            transcript_part_index,
            TranscriptPartSubType::Question,
            raw_request,
        );
        let response = "Phosphor AI Assistant cloud requests are disabled in Phosphor. Use Agent Mode with a configured BYOP model instead.".to_owned();
        let response_in_markdown = markdown_segments_from_text(
            transcript_part_index,
            TranscriptPartSubType::Answer,
            &response,
        );
        self.current_transcript.push(TranscriptPart {
            user: FormattedTranscriptMessage {
                markdown: request_in_markdown,
                raw: raw_request.to_string(),
            },
            assistant: AssistantTranscriptPart {
                is_error: true,
                copy_all_tooltip_and_button_mouse_handles: None,
                formatted_message: FormattedTranscriptMessage {
                    markdown: response_in_markdown,
                    raw: response,
                },
            },
        });
        self.request_status = RequestStatus::NotInFlight;
        send_telemetry_from_ctx!(
            TelemetryEvent::WarpAIRequestIssued {
                result: WarpAIRequestResult::Failed
            },
            ctx
        );
        ctx.emit(Event::RequestFinished { succeeded: false });
        ctx.notify();
    }

    pub fn reset(&mut self, ctx: &mut ModelContext<Self>) {
        if let RequestStatus::InFlight { abort_handle, .. } = &self.request_status {
            abort_handle.abort();
        }
        let mut old_transcript = Vec::new();
        std::mem::swap(&mut old_transcript, &mut self.current_transcript);
        self.old_transcript_parts.extend(old_transcript);
        self.request_status = RequestStatus::NotInFlight;
        self.current_transcript_summarized = false;
        ctx.notify();
    }

    pub fn transcript(&self) -> &[TranscriptPart] {
        self.current_transcript.as_slice()
    }

    /// Includes the old transcript parts appended with the current
    /// transcript parts. You likely want to just be using the current transcript parts
    /// (exposed by the `Requests::transcript` API) in most use cases.
    fn total_transcript_history(&self) -> impl Iterator<Item = &TranscriptPart> {
        self.old_transcript_parts
            .iter()
            .chain(self.current_transcript.iter())
    }

    pub fn all_past_transcript_prompts(&self) -> Vec<String> {
        self.total_transcript_history()
            .map(|p| p.raw_user_prompt().to_string())
            .collect()
    }

    pub fn request_status(&self) -> &RequestStatus {
        &self.request_status
    }

    pub fn current_transcript_summarized(&self) -> bool {
        self.current_transcript_summarized
    }

    /// Returns the number of remaining requests the user has based on their latest rate limit info.
    /// If the current time is past the next refresh time, then the number of remaining reqs is the limit.
    pub fn num_remaining_reqs(&self) -> usize {
        match self.remaining_time_to_refresh_std() {
            Err(_) => self.request_limit_info.limit,
            Ok(t) if t.is_zero() => self.request_limit_info.limit,
            Ok(_t) => {
                self.request_limit_info.limit
                    - self.request_limit_info.num_requests_used_since_refresh
            }
        }
    }

    pub fn num_requests_used(&self) -> usize {
        self.request_limit_info.limit - self.num_remaining_reqs()
    }

    pub fn request_limit(&self) -> usize {
        self.request_limit_info.limit
    }

    /// How long until the text from [`Self::serialized_time_until_refresh`] next changes, or
    /// `None` when no countdown is shown.  The text is floored to days, hours or minutes, so it
    /// only changes on a unit boundary (and when the countdown ends).
    pub fn time_until_countdown_text_changes(&self) -> Option<std::time::Duration> {
        match self.remaining_time_to_refresh_std() {
            Ok(t) if !t.is_zero() => Some(countdown_text_change_delay(t)),
            _ => None,
        }
    }

    /// Returns the next refresh time based on the latest rate limit info as a formatted string.
    /// If the current time is past the next refresh time, then returns None.
    pub fn serialized_time_until_refresh(&self) -> Option<String> {
        match self.remaining_time_to_refresh_std() {
            Err(_) => None,
            Ok(t) if t.is_zero() => None,
            Ok(t) => {
                let num_minutes = t.as_secs() / 60;
                let num_hours = num_minutes / 60;
                let num_days = num_hours / 24;
                let remaining_text = if num_days > 0 {
                    format!("{num_days} days")
                } else if num_hours > 0 {
                    format!("{num_hours} hours")
                } else {
                    format!("{num_minutes} minutes")
                };
                Some(remaining_text)
            }
        }
    }
}

#[cfg(test)]
impl Requests {
    pub fn new_with_transcript(transcript: Vec<TranscriptPart>) -> Self {
        Self {
            current_transcript: transcript,
            current_transcript_summarized: false,
            old_transcript_parts: Vec::new(),
            request_status: RequestStatus::NotInFlight,
            request_limit_info: RequestLimitInfo::default(),
        }
    }
}
