use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll},
};

use crate::{
    ParsedCommandsSnapshot, ParsedTokenData, ParsedTokensSnapshot,
    completer::{CompletionContext, describe_given_token},
    meta::{HasSpan as _, Span, SpannedItem},
    parsers::{LiteCommand, simple::all_parsed_commands},
};

/// How many tokens are described between cooperative yield points.
const TOKENS_PER_YIELD: usize = 8;

/// Returns `Pending` exactly once (waking itself), so an enclosing `Abortable` or
/// executor gets a chance to act between units of CPU-bound work.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Parse the current commands in the editor's buffer and get descriptions
/// for tokens within the commands. Note that this can be somewhat expensive,
/// which is why we execute this function asynchronously.
pub async fn parse_current_commands_and_tokens<T: CompletionContext>(
    buffer_text: String,
    completion_context: &T,
) -> ParsedTokensSnapshot {
    let never_cancelled = AtomicBool::new(false);
    parse_current_commands_and_tokens_cancellable(buffer_text, completion_context, &never_cancelled)
        .await
        .expect("a parse that is never cancelled always completes")
}

/// Like [`parse_current_commands_and_tokens`], but checks `cancel` between commands and
/// between tokens and returns `None` as soon as it is set. Aborting the enclosing future
/// cannot stop this work by itself: the per-token work is CPU-bound and may never return
/// `Pending`, so `Abortable` only notices the abort once the whole parse is done (#1031).
/// The function also yields to the executor every few tokens so an abort is observed
/// promptly even by callers that only abort and never set `cancel`.
pub async fn parse_current_commands_and_tokens_cancellable<T: CompletionContext>(
    buffer_text: String,
    completion_context: &T,
    cancel: &AtomicBool,
) -> Option<ParsedTokensSnapshot> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    // Parse commands
    let all_commands_iterator =
        all_parsed_commands(buffer_text.as_str(), completion_context.escape_char());
    // Note that we must collect the iterator into a vector to avoid referencing local data i.e. buffer_text within the future's output.
    let mut all_commands_vec: Vec<LiteCommand> = Vec::new();
    for command in all_commands_iterator {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        all_commands_vec.push(command);
    }
    let parsed_commands_snapshot = ParsedCommandsSnapshot {
        buffer_text: buffer_text.clone(),
        parsed_commands: all_commands_vec,
        completion_context,
    };
    // Get descriptions for tokens within commands.
    let parsed_tokens = get_token_descriptions(parsed_commands_snapshot, cancel).await?;

    Some(ParsedTokensSnapshot {
        buffer_text,
        parsed_tokens,
    })
}

/// Expands aliases in the provided snapshot iteratively until no aliases are found.
pub async fn expand_aliases<T: CompletionContext>(
    mut parsed_tokens_snapshot: ParsedTokensSnapshot,
    completion_context: &T,
) -> ParsedTokensSnapshot {
    // Perform up to three iterations of alias expansion (to hedge against recursive aliases).
    for _ in 0..3 {
        let mut expanded_buffer_text = String::new();
        let mut last_token_end = 0;
        for token in &parsed_tokens_snapshot.parsed_tokens {
            if token.token_index != 0 {
                continue;
            }

            let Some(alias) = completion_context.alias_command(token.token.as_str()) else {
                continue;
            };

            // Push any text between the last token and the current token.
            expanded_buffer_text.push_str(
                &parsed_tokens_snapshot.buffer_text[last_token_end..token.token.span().start()],
            );
            // Push the alias.
            expanded_buffer_text.push_str(alias);
            // Store the position in the buffer text that we've processed up to so far.
            last_token_end = token.token.span().end();
        }

        if expanded_buffer_text.is_empty() {
            // If we didn't find any aliases, we're done.
            break;
        } else {
            // Push any additional trailing content after the last expanded token.
            if last_token_end < parsed_tokens_snapshot.buffer_text.len() {
                expanded_buffer_text
                    .push_str(&parsed_tokens_snapshot.buffer_text[last_token_end..]);
            }
            // Update the parsed tokens snapshot.
            parsed_tokens_snapshot =
                parse_current_commands_and_tokens(expanded_buffer_text, completion_context).await;
        }
    }

    parsed_tokens_snapshot
}

/// Get a vector of parsed tokens data from a given parsed commands snapshot,
/// note that this is meant to run asynchronously.
async fn get_token_descriptions<'a, T: CompletionContext>(
    parsed_commands_snapshot: ParsedCommandsSnapshot<'a, T>,
    cancel: &AtomicBool,
) -> Option<Vec<ParsedTokenData>> {
    let buffer_text = parsed_commands_snapshot.buffer_text.as_str();
    let completion_context = parsed_commands_snapshot.completion_context;
    let mut parsed_token_data = Vec::new();
    let mut tokens_since_yield = 0usize;

    for parsed_command in parsed_commands_snapshot.parsed_commands {
        let current_command_span = parsed_command.span();

        for (token_index, token) in parsed_command.parts.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            tokens_since_yield += 1;
            if tokens_since_yield >= TOKENS_PER_YIELD {
                tokens_since_yield = 0;
                YieldNow(false).await;
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
            }
            // Split --flag=value tokens into separate flag-name and value entries so that each
            // part gets its own syntax highlighting color and description.
            let eq_split = token
                .item
                .starts_with('-')
                .then(|| token.item.find('='))
                .flatten();

            if let Some(eq_pos) = eq_split {
                let eq_byte_pos = token.span.start() + eq_pos;

                let flag_token = token.item[..eq_pos]
                    .to_string()
                    .spanned(Span::new(token.span.start(), eq_byte_pos));
                let flag_description = describe_given_token(
                    buffer_text,
                    &current_command_span,
                    flag_token.clone(),
                    completion_context,
                )
                .await;
                parsed_token_data.push(ParsedTokenData {
                    token: flag_token,
                    token_index,
                    token_description: flag_description,
                });

                let value_token = token.item[eq_pos + 1..]
                    .to_string()
                    .spanned(Span::new(eq_byte_pos + 1, token.span.end()));
                if !value_token.item.is_empty() {
                    let value_description = describe_given_token(
                        buffer_text,
                        &current_command_span,
                        value_token.clone(),
                        completion_context,
                    )
                    .await;
                    parsed_token_data.push(ParsedTokenData {
                        token: value_token,
                        token_index,
                        token_description: value_description,
                    });
                }
            } else {
                let token_description = describe_given_token(
                    buffer_text,
                    &current_command_span,
                    token.clone(),
                    completion_context,
                )
                .await;

                // Note that this is the token index relative to the current command meaning that
                // in the final flattened vector, we could have multiple tokens with index 0.
                parsed_token_data.push(ParsedTokenData {
                    token,
                    token_index,
                    token_description,
                })
            }
        }
    }

    Some(parsed_token_data)
}

#[cfg(test)]
mod tests {
    use std::{
        pin::pin,
        sync::atomic::{AtomicBool, Ordering},
        task::{Context, Poll},
    };

    use futures::task::noop_waker_ref;

    use super::*;
    use crate::{completer::testing::FakeCompletionContext, signatures::CommandRegistry};

    fn context() -> FakeCompletionContext {
        FakeCompletionContext::new(CommandRegistry::default()).with_top_level_commands(vec!["echo"])
    }

    fn many_words(n: usize) -> String {
        format!("echo {}", vec!["word"; n].join(" "))
    }

    #[test]
    fn already_cancelled_parse_does_no_work() {
        let ctx = context();
        let cancel = AtomicBool::new(true);
        let result = warpui::r#async::block_on(parse_current_commands_and_tokens_cancellable(
            many_words(200),
            &ctx,
            &cancel,
        ));
        assert!(result.is_none());
    }

    #[test]
    fn uncancelled_parse_describes_every_token() {
        let ctx = context();
        let cancel = AtomicBool::new(false);
        let snapshot = warpui::r#async::block_on(parse_current_commands_and_tokens_cancellable(
            many_words(40),
            &ctx,
            &cancel,
        ))
        .expect("not cancelled");
        assert_eq!(snapshot.parsed_tokens.len(), 41);
    }

    /// A superseded job stops at its next yield point instead of describing every token:
    /// the future yields (`Pending`) every few tokens, so after `cancel` is set the very
    /// next poll resolves to `None` rather than finishing the remaining ~1000 tokens.
    #[test]
    fn cancelling_mid_parse_stops_at_next_yield() {
        let ctx = context();
        let cancel = AtomicBool::new(false);
        let mut future = pin!(parse_current_commands_and_tokens_cancellable(
            many_words(1000),
            &ctx,
            &cancel,
        ));
        let mut cx = Context::from_waker(noop_waker_ref());

        assert!(
            future.as_mut().poll(&mut cx).is_pending(),
            "a large parse must yield instead of running to completion in one poll"
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(
            matches!(future.as_mut().poll(&mut cx), Poll::Ready(None)),
            "the cancelled parse must resolve to None at its next yield"
        );
    }
}
