use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use chrono::Local;
use warp_multi_agent_api::{FileContent, FileContentLineRange};

use crate::ai::agent::{
    AIAgentContext, AIAgentExchange, AIAgentExchangeId, AIAgentOutput, AIAgentOutputMessage,
    AIAgentOutputMessageType, AIAgentOutputStatus, AIAgentText, AIAgentTextSection,
    AgentOutputImage, AgentOutputImageLayout, AgentOutputMermaidDiagram, AnyFileContent,
    CancellationReason, FileContext, FinishedAIAgentOutput, FormattedTextWrapper, MessageId,
    ProgrammingLanguage, RenderableAIError, Shared, TransientNetworkErrorKind,
};
use crate::ai::llms::LLMId;
use crate::terminal::shell::ShellType;
use markdown_parser::{FormattedText, FormattedTextFragment, FormattedTextLine};

/// Builds a minimal exchange with no input and the given `output_status`, for testing
/// `AIAgentExchange::format_output_for_copy`. Mirrors `create_test_exchange` in
/// `task_store_tests.rs`.
fn exchange_with_output_status(output_status: AIAgentOutputStatus) -> AIAgentExchange {
    AIAgentExchange {
        id: AIAgentExchangeId::new(),
        input: vec![],
        output_status,
        added_message_ids: HashSet::new(),
        start_time: Local::now(),
        finish_time: None,
        time_to_first_token_ms: None,
        working_directory: None,
        model_id: LLMId::from("test-model"),
        request_cost: None,
        coding_model_id: LLMId::from("test-coding-model"),
        cli_agent_model_id: LLMId::from("test-cli-agent-model"),
        computer_use_model_id: LLMId::from("test-computer-use-model"),
        response_initiator: None,
    }
}

fn to_range(range: Range<u32>) -> Option<FileContentLineRange> {
    Some(FileContentLineRange {
        start: range.start,
        end: range.end,
    })
}

#[test]
fn formatted_text_wrapper_shares_arc_across_calls() {
    let text = FormattedText::new([FormattedTextLine::Line(vec![
        FormattedTextFragment::plain_text("hello world"),
    ])]);
    let wrapper = FormattedTextWrapper::from(text);
    let arc1 = wrapper.formatted_text_arc();
    let arc2 = wrapper.formatted_text_arc();
    // Both calls must return the same allocation — not independent deep copies.
    assert!(Arc::ptr_eq(&arc1, &arc2));
}

#[test]
fn formatted_text_wrapper_preserves_content() {
    let text = FormattedText::new([
        FormattedTextLine::Line(vec![FormattedTextFragment::plain_text("line one")]),
        FormattedTextLine::Line(vec![FormattedTextFragment::plain_text("line two")]),
    ]);
    let wrapper = FormattedTextWrapper::from(text);
    // lines() metadata matches the cached Arc
    assert_eq!(wrapper.lines().len(), 2);
    assert_eq!(wrapper.lines()[0].raw_text(), "line one\n");
    assert_eq!(wrapper.lines()[1].raw_text(), "line two\n");
    // Arc contains the same lines
    let ft = wrapper.formatted_text_arc();
    assert_eq!(ft.lines.len(), 2);
}

#[test]
fn test_convert_files() {
    let a = FileContext::new(
        "a.txt".to_string(),
        AnyFileContent::StringContent("hey\nyou".to_string()),
        None,
        None,
    );

    assert_eq!(
        Into::<Vec<FileContent>>::into(a),
        vec![FileContent {
            file_path: "a.txt".to_string(),
            content: "hey\nyou".to_string(),
            line_range: None,
        }]
    );
}

#[test]
fn test_convert_files_range() {
    // Content is pre-sliced to match the line range.
    let a = FileContext::new(
        "a.txt".to_string(),
        AnyFileContent::StringContent("hey\nyou".to_string()),
        Some(1..2),
        None,
    );

    assert_eq!(
        Into::<Vec<FileContent>>::into(a),
        vec![FileContent {
            file_path: "a.txt".to_string(),
            content: "hey\nyou".to_string(),
            line_range: to_range(1..2),
        }]
    );
}

#[test]
fn test_convert_files_range_out_of_bounds() {
    // Even with an out-of-bounds range, content is passed through as-is.
    let a = FileContext::new(
        "a.txt".to_string(),
        AnyFileContent::StringContent(String::new()),
        Some(10..20),
        None,
    );

    assert_eq!(
        Into::<Vec<FileContent>>::into(a),
        vec![FileContent {
            file_path: "a.txt".to_string(),
            content: String::new(),
            line_range: to_range(10..20),
        }]
    );
}

#[test]
fn test_programming_language_from_string() {
    // Shell language specifiers should produce Shell variants
    assert_eq!(
        ProgrammingLanguage::from("bash".to_string()),
        ProgrammingLanguage::Shell(ShellType::Bash)
    );
    assert_eq!(
        ProgrammingLanguage::from("shell".to_string()),
        ProgrammingLanguage::Shell(ShellType::Bash)
    );
    assert_eq!(
        ProgrammingLanguage::from("sh".to_string()),
        ProgrammingLanguage::Shell(ShellType::Bash)
    );
    assert_eq!(
        ProgrammingLanguage::from("zsh".to_string()),
        ProgrammingLanguage::Shell(ShellType::Zsh)
    );
    assert_eq!(
        ProgrammingLanguage::from("fish".to_string()),
        ProgrammingLanguage::Shell(ShellType::Fish)
    );
    assert_eq!(
        ProgrammingLanguage::from("powershell".to_string()),
        ProgrammingLanguage::Shell(ShellType::PowerShell)
    );
    assert_eq!(
        ProgrammingLanguage::from("pwsh".to_string()),
        ProgrammingLanguage::Shell(ShellType::PowerShell)
    );

    // Non-shell languages should produce Other variants
    assert_eq!(
        ProgrammingLanguage::from("python".to_string()),
        ProgrammingLanguage::Other("python".to_string())
    );
    assert_eq!(
        ProgrammingLanguage::from("rust".to_string()),
        ProgrammingLanguage::Other("rust".to_string())
    );
    assert_eq!(
        ProgrammingLanguage::from("javascript".to_string()),
        ProgrammingLanguage::Other("javascript".to_string())
    );
}

#[test]
fn test_programming_language_to_extension() {
    // Each entry is (markdown language token, expected extension). The expected extension
    // must resolve back to a recognized language via `languages::language_by_filename` so that
    // syntax highlighting is applied to the AI block.
    let cases: &[(&str, &str)] = &[
        // Canonical names.
        ("rust", "rs"),
        ("go", "go"),
        ("python", "py"),
        ("javascript", "js"),
        ("typescript", "ts"),
        ("yaml", "yaml"),
        ("cpp", "cpp"),
        ("java", "java"),
        ("c#", "cs"),
        ("csharp", "cs"),
        ("html", "html"),
        ("css", "css"),
        ("c", "c"),
        ("json", "json"),
        ("hcl", "hcl"),
        ("lua", "lua"),
        ("ruby", "rb"),
        ("php", "php"),
        ("toml", "toml"),
        ("swift", "swift"),
        ("kotlin", "kt"),
        ("powershell", "ps1"),
        ("elixir", "exs"),
        ("scala", "scala"),
        ("sql", "sql"),
        // Languages newly covered by this fix — previously fell through to None and rendered
        // without syntax highlighting in AI blocks even though the `languages` crate supports them.
        ("jsx", "jsx"),
        ("tsx", "tsx"),
        ("xml", "xml"),
        ("vue", "vue"),
        ("dockerfile", "dockerfile"),
        ("starlark", "bzl"),
        ("objective-c", "m"),
        ("objc", "m"),
        // Common markdown code-fence aliases.
        ("rs", "rs"),
        ("golang", "go"),
        ("py", "py"),
        ("js", "js"),
        ("ts", "ts"),
        ("yml", "yaml"),
        ("c++", "cpp"),
        ("rb", "rb"),
        ("kt", "kt"),
        ("terraform", "hcl"),
        ("tf", "hcl"),
        ("docker", "dockerfile"),
        ("containerfile", "dockerfile"),
        ("markdown", "md"),
        ("md", "md"),
    ];
    for (token, expected_extension) in cases {
        let language = ProgrammingLanguage::from((*token).to_string());
        assert_eq!(
            language.to_extension(),
            Some(*expected_extension),
            "expected to_extension({token:?}) to be Some({expected_extension:?})",
        );
    }

    // PowerShell remains the only Shell variant whose extension is exposed; this preserves
    // existing behavior for the other Shell variants which are intentionally not extended here.
    assert_eq!(
        ProgrammingLanguage::Shell(ShellType::PowerShell).to_extension(),
        Some("ps1"),
    );

    // Unrecognized tokens still return None.
    assert_eq!(
        ProgrammingLanguage::Other("definitely-not-a-language".to_string()).to_extension(),
        None,
    );
}

#[test]
fn format_for_copy_preserves_visual_markdown_sections() {
    let output = AIAgentOutput {
        messages: vec![AIAgentOutputMessage {
            id: MessageId::new("message-1".to_string()),
            message: AIAgentOutputMessageType::Text(AIAgentText {
                sections: vec![
                    AIAgentTextSection::PlainText {
                        text: "Intro".to_string().into(),
                    },
                    AIAgentTextSection::Image {
                        image: AgentOutputImage {
                            alt_text: "Diagram".to_string(),
                            source: "./diagram.png".to_string(),
                            title: None,
                            markdown_source: "![Diagram](./diagram.png)".to_string(),
                            layout: AgentOutputImageLayout::Block,
                        },
                    },
                    AIAgentTextSection::MermaidDiagram {
                        diagram: AgentOutputMermaidDiagram {
                            source: "graph TD\nA --> B".to_string(),
                            markdown_source: "```mermaid\ngraph TD\nA --> B\n```".to_string(),
                        },
                    },
                ],
            }),
            citations: Vec::new(),
        }],
        ..Default::default()
    };

    assert_eq!(
        output.format_for_copy(None),
        "Intro\n![Diagram](./diagram.png)\n```mermaid\ngraph TD\nA --> B\n```"
    );
}

// Regression tests for the "Copy output as Markdown" silent-empty-clipboard bug: an exchange
// that ended in `Error` (or was `Cancelled` before any output streamed) used to make
// `format_output_for_copy` return `""` via `AIAgentOutputStatus::output() == None`, which is
// indistinguishable from "nothing happened yet". For a copy action, the error/cancellation is
// exactly the information the user wants, so it must be included.

#[test]
fn format_output_for_copy_surfaces_error_message_for_errored_exchange_with_no_partial_output() {
    let exchange = exchange_with_output_status(AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Error {
            output: None,
            error: RenderableAIError::Other {
                error_message: "the provider returned a 500".to_string(),
                will_attempt_resume: false,
                waiting_for_network: false,
            },
        },
    });

    let copied = exchange.format_output_for_copy(None);

    assert!(
        !copied.trim().is_empty(),
        "an errored exchange must contribute non-empty copy text, got {copied:?}"
    );
    assert!(
        copied.contains("the provider returned a 500"),
        "copy text should surface the actual error message, got {copied:?}"
    );

    // `AIAgentOutputStatus::output()` must still report `None` for this exchange — other
    // callers (rendering, status checks) rely on that meaning "nothing was streamed", and
    // this fix must not repurpose it.
    assert!(exchange.output_status.output().is_none());
}

#[test]
fn format_output_for_copy_includes_partial_output_alongside_the_error() {
    let partial_output = Shared::new(AIAgentOutput {
        messages: vec![AIAgentOutputMessage {
            id: MessageId::new("message-1".to_string()),
            message: AIAgentOutputMessageType::Text(AIAgentText {
                sections: vec![AIAgentTextSection::PlainText {
                    text: "partial progress before the failure".to_string().into(),
                }],
            }),
            citations: Vec::new(),
        }],
        ..Default::default()
    });

    let exchange = exchange_with_output_status(AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Error {
            output: Some(partial_output),
            error: RenderableAIError::Other {
                error_message: "stream cut off".to_string(),
                will_attempt_resume: false,
                waiting_for_network: false,
            },
        },
    });

    let copied = exchange.format_output_for_copy(None);

    assert!(
        copied.contains("partial progress before the failure"),
        "partial output streamed before the error must still be included, got {copied:?}"
    );
    assert!(
        copied.contains("stream cut off"),
        "the error must be appended alongside the partial output, got {copied:?}"
    );
}

#[test]
fn format_output_for_copy_notes_cancellation_when_no_output_was_ever_streamed() {
    let exchange = exchange_with_output_status(AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Cancelled {
            output: None,
            reason: CancellationReason::ManuallyCancelled,
        },
    });

    let copied = exchange.format_output_for_copy(None);

    assert!(
        !copied.trim().is_empty(),
        "a cancelled exchange with no output must still contribute copy text, got {copied:?}"
    );
    assert!(
        copied.contains("manual cancellation"),
        "copy text should say why the exchange has no output, got {copied:?}"
    );
}

#[test]
fn format_output_for_copy_ignores_cancellation_reason_when_output_was_streamed() {
    // A cancellation that *did* produce output before stopping already has real content to
    // copy, so it must not be decorated with a redundant cancellation annotation.
    let partial_output = Shared::new(AIAgentOutput {
        messages: vec![AIAgentOutputMessage {
            id: MessageId::new("message-1".to_string()),
            message: AIAgentOutputMessageType::Text(AIAgentText {
                sections: vec![AIAgentTextSection::PlainText {
                    text: "here is what I found".to_string().into(),
                }],
            }),
            citations: Vec::new(),
        }],
        ..Default::default()
    });

    let exchange = exchange_with_output_status(AIAgentOutputStatus::Finished {
        finished_output: FinishedAIAgentOutput::Cancelled {
            output: Some(partial_output),
            reason: CancellationReason::ManuallyCancelled,
        },
    });

    assert_eq!(
        exchange.format_output_for_copy(None),
        "here is what I found"
    );
}

#[test]
fn transient_network_error_includes_user_facing_message_and_debug_details() {
    // BYOP adaptation: `AIApiError` is neither `Clone` nor serializable, so the
    // structured cause is carried as its debug-rendered string (mirroring Warp's
    // `Api(Arc<AIApiError>)` rendered via `{0:?}`).
    let error = RenderableAIError::transient_network_error(
        false,
        false,
        TransientNetworkErrorKind::Api("connection reset".to_string()),
    );

    let rendered = error.to_string();
    assert!(
        rendered.starts_with(
            "Zap lost connection while receiving the agent response. This is usually temporary.\n\nDebug info: "
        ),
        "unexpected rendering: {rendered}"
    );
    // The raw underlying API error must survive into the debug section.
    assert!(
        rendered.contains("connection reset"),
        "raw error detail should surface in debug info: {rendered}"
    );
    assert!(!error.will_attempt_resume());
}

#[test]
fn transient_network_error_reports_pending_resume() {
    let error = RenderableAIError::transient_network_error(
        true,
        false,
        TransientNetworkErrorKind::Api("connection reset".to_string()),
    );

    assert!(error.will_attempt_resume());
}

// ─── Ported from the pinned oracle ───────────────────────────────────────────
// `02b53fcd8:app/src/ai/agent/mod_tests.rs`

fn deserialize_pull_request_number_from_json(number_json: &str) -> serde_json::Result<i32> {
    let context = serde_json::from_str::<AIAgentContext>(&format!(
        r#"{{"PullRequest":{{"number":{number_json}}}}}"#
    ))?;
    match context {
        AIAgentContext::PullRequest { number, .. } => Ok(number),
        other => panic!("expected pull request context, got {other:?}"),
    }
}

#[test]
fn pull_request_number_deserializer_accepts_positive_number_and_string() {
    assert_eq!(deserialize_pull_request_number_from_json("42").unwrap(), 42);
    assert_eq!(
        deserialize_pull_request_number_from_json(r#""42""#).unwrap(),
        42
    );
}

#[test]
fn pull_request_number_deserializer_defaults_invalid_numbers() {
    for number_json in ["null", "0", "-1", "1.5", "2147483648", r#""""#, r#""abc""#] {
        assert_eq!(
            deserialize_pull_request_number_from_json(number_json).unwrap(),
            0,
            "expected {number_json} to deserialize to default pull request number",
        );
    }
}

#[test]
fn pull_request_number_deserializer_rejects_unsupported_json_types() {
    for number_json in ["true", "[]", "{}"] {
        assert!(
            deserialize_pull_request_number_from_json(number_json).is_err(),
            "expected {number_json} to fail deserialization",
        );
    }
}
