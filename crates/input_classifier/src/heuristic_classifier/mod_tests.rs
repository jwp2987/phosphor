use warp_completer::meta::SpannedItem;
use warp_completer::util::parse_current_commands_and_tokens;
use warp_completer::{ParsedTokenData, ParsedTokensSnapshot};

use crate::{Context, test_utils::CompletionContext};

use super::*;

async fn mock_parsed_input_token(buffer_text: String) -> ParsedTokensSnapshot {
    let completion_context = CompletionContext::new();
    parse_current_commands_and_tokens(buffer_text, &completion_context).await
}

fn mock_parsed_input_token_without_descriptions(buffer_text: &str) -> ParsedTokensSnapshot {
    let mut next_search_start = 0;
    let parsed_tokens = buffer_text
        .split_whitespace()
        .enumerate()
        .map(|(token_index, token)| {
            let token_start =
                buffer_text[next_search_start..].find(token).unwrap() + next_search_start;
            let token_end = token_start + token.len();
            next_search_start = token_end;

            ParsedTokenData {
                token: token.to_string().spanned((token_start, token_end)),
                token_index,
                token_description: None,
            }
        })
        .collect();

    ParsedTokensSnapshot {
        buffer_text: buffer_text.to_string(),
        parsed_tokens,
    }
}

#[test]
fn test_input_detection() {
    futures::executor::block_on(async move {
        let classifier = HeuristicClassifier;

        let mut context = Context {
            current_input_type: InputType::AI,
            is_agent_follow_up: false,
        };

        let token = mock_parsed_input_token("cargo --version".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::Shell
        );

        // We have to override the first token description here given the mocked completion
        // parser will parse the first token always as commands.
        //
        // Mock the case where cargo is not installed. We should still parse this as Shell input.
        let mut token = mock_parsed_input_token("cargo --version".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::Shell
        );

        let mut token = mock_parsed_input_token("rvm install 3.3".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::Shell
        );

        // Short queries with NL should be parsed as AI input when already in AI input.
        let mut token = mock_parsed_input_token("Explain this".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token.clone(), &context).await.input_type,
            InputType::AI
        );

        context.current_input_type = InputType::Shell;

        // Typing "fix this" after an error block is a common use case.
        let mut token = mock_parsed_input_token("fix this".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI,
        );

        // Short queries with punctuation should be parsed as AI input.
        let token = mock_parsed_input_token("What went wrong?".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );
        // Short queries with contractions should be parsed as AI input.
        let mut token = mock_parsed_input_token("What's the reason".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Short queries with quotations should be parsed as AI input.
        let mut token =
            mock_parsed_input_token("The message is \"utils::future ... ok\"".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // String tokens with special shell syntax should not be treated as negative NL signal.
        let mut token = mock_parsed_input_token("The type is \"<>\"".to_string()).await;
        token.parsed_tokens[0].token_description = None;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );
    });
}

#[test]
fn test_cjk_input_detection() {
    futures::executor::block_on(async move {
        let classifier = HeuristicClassifier;

        // Starting from Shell mode by default (stricter scenario, verifying CJK can still switch to AI).
        let context = Context {
            current_input_type: InputType::Shell,
            is_agent_follow_up: false,
        };

        // A single CJK character also classifies as AI (default logic would classify it as Shell due to token count < 2).
        let token = mock_parsed_input_token("帮我列出当前目录文件".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Mixed Chinese and English; as long as it contains CJK, it goes to AI.
        let token = mock_parsed_input_token("用 cargo build 编译这个项目".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Chinese punctuation (fullwidth comma, period, question mark) also matches.
        let token = mock_parsed_input_token("这是什么?".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Japanese (hiragana + katakana).
        let token = mock_parsed_input_token("ファイルを表示してください".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Korean.
        let token = mock_parsed_input_token("파일 목록을 보여줘".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::AI
        );

        // Pure English shell commands are unaffected.
        let token = mock_parsed_input_token("ls -la".to_string()).await;
        assert_eq!(
            classifier.detect_input_type(token, &context).await.input_type,
            InputType::Shell
        );
    });
}

#[test]
fn test_input_detection_sources() {
    futures::executor::block_on(async move {
        let classifier = HeuristicClassifier;
        let context = Context {
            current_input_type: InputType::Shell,
            is_agent_follow_up: false,
        };

        let token = mock_parsed_input_token_without_descriptions("echo hello");
        let decision = classifier.detect_input_type(token, &context).await;
        assert_eq!(
            decision,
            InputClassificationResult::new(
                InputType::Shell,
                InputClassifierDecisionSource::ShellHeuristic,
            )
        );
        let token = mock_parsed_input_token_without_descriptions("explain");
        let decision = classifier.detect_input_type(token, &context).await;
        assert_eq!(
            decision,
            InputClassificationResult::new(
                InputType::AI,
                InputClassifierDecisionSource::NaturalLanguageOneOffAllowlist,
            )
        );
        let token = mock_parsed_input_token_without_descriptions("fix this");
        let decision = classifier.detect_input_type(token, &context).await;
        assert_eq!(
            decision,
            InputClassificationResult::new(
                InputType::AI,
                InputClassifierDecisionSource::InputClassifierFallbackHeuristic,
            )
        );
    });
}
