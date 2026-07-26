//! Infers which multimodal attachment types a model supports in BYOP mode, keyed by
//! `api_type` × `model_id`.
//!
//! genai 0.6's `ContentPart::Binary` is fully auto-adapted at the wire protocol
//! layer (see the comment table in `chat_stream.rs`):
//! - OpenAI: image→`image_url{data:URL}`, pdf/file→`type:"file" file_data:data:URL`, audio→`input_audio`
//! - Anthropic: image→`image base64`, other→`document base64` (in practice only PDF works)
//! - Gemini: everything goes through `inline_data`
//!
//! But **wire-protocol support** ≠ **model support**. This module only holds the
//! determination of "what the model can actually consume", to avoid sending images
//! to a text-only model like GPT-3.5 or Claude Sonnet 1.0 and getting an upstream
//! error.
//!
//! The determination goes through model_id substring matching, in the same style as
//! `prompt_renderer::resolve_template`. The substring rules are deliberately loose
//! (containing the substring counts as a match); the goal is to "cover future minor
//! upgrades within the same family" rather than "enumerate exact versions" — weighing
//! false-positive risk against maintenance cost, the tradeoff leans toward keeping
//! maintenance cost low.

use super::models_dev;
use crate::settings::{AgentProviderApiType, AgentProviderModel};

/// A table of a model's support for attachment types.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttachmentCaps {
    /// Whether images are supported (image/* MIME).
    pub images: bool,
    /// Whether PDF is supported (application/pdf MIME).
    pub pdf: bool,
    /// Whether audio is supported (audio/* MIME).
    pub audio: bool,
}

impl AttachmentCaps {
    /// No multimodal capability at all → upstream must fall back to the text-only
    /// path.
    pub fn is_text_only(&self) -> bool {
        !self.images && !self.pdf && !self.audio
    }

    /// Given a mime, asks whether this model can consume this binary attachment.
    pub fn supports_mime(&self, mime: &str) -> bool {
        let lower = mime.trim().to_ascii_lowercase();
        if lower.starts_with("image/") {
            return self.images;
        }
        if lower == "application/pdf" {
            return self.pdf;
        }
        if lower.starts_with("audio/") {
            return self.audio;
        }
        false
    }
}

/// First checks the models.dev catalog; falls back to (api_type, model_id
/// substring) when the catalog misses.
///
/// The catalog is the authoritative source for real model capabilities (fetched
/// when the user clicks "Sync from models.dev" in settings, or via the 24h
/// auto-refresh); the fallback rules ensure mainstream models still work offline or
/// before the catalog has been fetched.
pub fn caps_for(api_type: AgentProviderApiType, model_id: &str) -> AttachmentCaps {
    if let Some(c) = models_dev::lookup_caps("", model_id) {
        return AttachmentCaps {
            images: c.vision,
            pdf: c.pdf,
            audio: c.audio,
        };
    }
    caps_for_by_substring(api_type, model_id)
}

/// Resolves the final capability for a single model, **with the user's three-state
/// override**. Three-tier priority:
/// 1. The user's explicit `Some(_)` in settings → used directly, bypassing inference
/// 2. `None` → inferred from the models.dev catalog
/// 3. catalog miss → substring fallback
///
/// `provider_id` is used for exact provider matching in the catalog (to handle the
/// special path for aggregator providers like OpenRouter); the fallback path doesn't
/// need provider_id when the catalog misses.
pub fn resolve_for_model(
    provider_id: &str,
    api_type: AgentProviderApiType,
    model: &AgentProviderModel,
) -> AttachmentCaps {
    let inferred = if let Some(c) = models_dev::lookup_caps(provider_id, &model.id) {
        AttachmentCaps {
            images: c.vision,
            pdf: c.pdf,
            audio: c.audio,
        }
    } else {
        caps_for_by_substring(api_type, &model.id)
    };
    AttachmentCaps {
        images: model.image.unwrap_or(inferred.images),
        pdf: model.pdf.unwrap_or(inferred.pdf),
        audio: model.audio.unwrap_or(inferred.audio),
    }
}

/// A snapshot of the "inference result" for UI use (ignores the user's override,
/// only looks at catalog/fallback).
/// Used to display "Auto: catalog says supported" semantics in the chip tooltip.
pub fn inferred_for_model(
    provider_id: &str,
    api_type: AgentProviderApiType,
    model_id: &str,
) -> AttachmentCaps {
    if let Some(c) = models_dev::lookup_caps(provider_id, model_id) {
        AttachmentCaps {
            images: c.vision,
            pdf: c.pdf,
            audio: c.audio,
        }
    } else {
        caps_for_by_substring(api_type, model_id)
    }
}

/// Falls back to a table lookup keyed by (api_type, model_id substring).
///
/// By default, returns "all false" conservatively for any unknown model — the
/// benefit is that we never mistakenly stuff binary data into an unsupported model
/// and cause a 400; the cost is that new models need to be added manually when they
/// launch (acceptable, since every new model needs other config updated anyway, like
/// reasoning_effort / context_window).
fn caps_for_by_substring(api_type: AgentProviderApiType, model_id: &str) -> AttachmentCaps {
    let lower = model_id.to_ascii_lowercase();
    match api_type {
        AgentProviderApiType::OpenAi | AgentProviderApiType::OpenAiResp => {
            // GPT-4o / 4.1 / 5 series: image + pdf. The 3.5 series is text-only.
            if lower.contains("gpt-4o")
                || lower.contains("gpt-4.1")
                || lower.contains("gpt-5")
                || lower.contains("o1")
                || lower.contains("o3")
                || lower.contains("o4")
            {
                AttachmentCaps {
                    images: true,
                    pdf: true,
                    audio: false,
                }
            } else if lower.contains("gpt-4o-audio") || lower.contains("gpt-realtime") {
                AttachmentCaps {
                    images: true,
                    pdf: true,
                    audio: true,
                }
            } else {
                AttachmentCaps::default()
            }
        }
        AgentProviderApiType::Anthropic => {
            // All of Claude 3 / 3.5 / 4 / 4.5 / 4.7 support vision + document (PDF).
            if lower.contains("claude-3")
                || lower.contains("claude-4")
                || lower.contains("claude-opus")
                || lower.contains("claude-sonnet")
                || lower.contains("claude-haiku")
            {
                AttachmentCaps {
                    images: true,
                    pdf: true,
                    audio: false,
                }
            } else {
                AttachmentCaps::default()
            }
        }
        AgentProviderApiType::Gemini => {
            // All of Gemini 1.5+ / 2 / 2.5 are multimodal; inline_data supports
            // image/pdf/audio/video.
            if lower.contains("gemini-1.5")
                || lower.contains("gemini-2")
                || lower.contains("gemini-pro-vision")
            {
                AttachmentCaps {
                    images: true,
                    pdf: true,
                    audio: true,
                }
            } else {
                AttachmentCaps::default()
            }
        }
        AgentProviderApiType::Ollama => {
            // Most Ollama models are text-only. Vision models (LLaVA / bakllava /
            // llama3.2-vision / qwen2-vl / minicpm-v / moondream) get image
            // capability turned on via model_id substring matching.
            // PDF/audio are essentially unsupported under the Ollama protocol, so we
            // conservatively return false.
            let vision = lower.contains("llava")
                || lower.contains("bakllava")
                || lower.contains("vision")
                || lower.contains("-vl")
                || lower.contains("minicpm-v")
                || lower.contains("moondream");
            AttachmentCaps {
                images: vision,
                pdf: false,
                audio: false,
            }
        }
        AgentProviderApiType::DeepSeek => {
            // DeepSeek's currently public models (v3/r1/coder/chat) are all
            // text-only for now.
            // Enable this once a future deepseek-vl series launches.
            if lower.contains("vl") {
                AttachmentCaps {
                    images: true,
                    pdf: false,
                    audio: false,
                }
            } else {
                AttachmentCaps::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_4o_supports_image_and_pdf() {
        // Exercises the fallback rules: in the test environment the models.dev
        // catalog isn't loaded, so lookup_caps returns None.
        let caps = caps_for_by_substring(AgentProviderApiType::OpenAi, "gpt-4o-2024-08-06");
        assert!(caps.images);
        assert!(caps.pdf);
        assert!(!caps.audio);
    }

    #[test]
    fn openai_3_5_text_only() {
        let caps = caps_for_by_substring(AgentProviderApiType::OpenAi, "gpt-3.5-turbo");
        assert!(caps.is_text_only());
    }

    #[test]
    fn claude_sonnet_supports_image_and_pdf() {
        let caps = caps_for_by_substring(AgentProviderApiType::Anthropic, "claude-sonnet-4-5");
        assert!(caps.images);
        assert!(caps.pdf);
    }

    #[test]
    fn gemini_2_5_full_multimodal() {
        let caps = caps_for_by_substring(AgentProviderApiType::Gemini, "gemini-2.5-pro");
        assert!(caps.images);
        assert!(caps.pdf);
        assert!(caps.audio);
    }

    #[test]
    fn ollama_default_text_only() {
        let caps = caps_for_by_substring(AgentProviderApiType::Ollama, "qwen2.5:7b");
        assert!(caps.is_text_only());
    }

    #[test]
    fn ollama_vision_models_get_images() {
        let caps = caps_for_by_substring(AgentProviderApiType::Ollama, "llava:13b");
        assert!(caps.images);
        assert!(!caps.pdf);
    }

    #[test]
    fn deepseek_chat_text_only() {
        let caps = caps_for_by_substring(AgentProviderApiType::DeepSeek, "deepseek-chat");
        assert!(caps.is_text_only());
    }

    #[test]
    fn supports_mime_routing() {
        let full = AttachmentCaps {
            images: true,
            pdf: true,
            audio: true,
        };
        assert!(full.supports_mime("image/png"));
        assert!(full.supports_mime("application/pdf"));
        assert!(full.supports_mime("audio/mp3"));
        assert!(!full.supports_mime("application/zip"));
        assert!(!full.supports_mime("text/plain"));
    }
}
