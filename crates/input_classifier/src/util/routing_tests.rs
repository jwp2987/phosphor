//! Direct tests of the #1980 rule (a mistyped command in an agent conversation) and of the #2113
//! rule (PowerShell expression heads). The corpus in `routing_corpus.rs` covers every other rule.

use super::*;

fn unparsed(text: &str) -> ParsedTokensSnapshot {
    ParsedTokensSnapshot {
        buffer_text: text.to_owned(),
        parsed_tokens: Vec::new(),
    }
}

/// The facts for an unknown head: `absent` is the session saying the word is no command (Enter),
/// otherwise it cannot say (the keystroke, or an Enter before the session's data landed).
fn context(surface: Surface, absent: bool, near_installed: bool) -> RoutingContext {
    RoutingContext {
        surface,
        shell: ShellFamily::Posix,
        head_known_to_shell: absent.then_some(false),
        commands_fully_loaded: false,
        command_data_current: false,
        is_agent_follow_up: true,
        ran_before_exactly: false,
        denylisted: false,
        typeahead: Typeahead::None,
        head_near_installed_command: near_installed,
    }
}

#[test]
fn one_edit_apart_is_one_substitution_insertion_deletion_or_swap() {
    for (a, b) in [
        ("gti", "git"),
        ("gitt", "git"),
        ("gt", "git"),
        ("it", "git"),
        ("gi", "git"),
        ("gir", "git"),
        ("xit", "git"),
        ("dokcer", "docker"),
        ("gerp", "grep"),
        ("mkdri", "mkdir"),
        ("kubctl", "kubectl"),
        ("a", ""),
    ] {
        assert!(is_one_edit_apart(a, b), "{a:?} / {b:?}");
        assert!(is_one_edit_apart(b, a), "{b:?} / {a:?}");
    }
    for (a, b) in [
        ("git", "git"),
        ("", ""),
        ("tig", "git"),
        ("gti", "gits"),
        ("gitxx", "git"),
        ("g", "git"),
        ("grpe", "gerp"),
        ("abcd", "badc"),
        ("status", "git"),
    ] {
        assert!(!is_one_edit_apart(a, b), "{a:?} / {b:?}");
        assert!(!is_one_edit_apart(b, a), "{b:?} / {a:?}");
    }
}

/// Every CLI the rule compares a head with has a subcommand list, and none is shorter than the
/// minimum the rule compares.
#[test]
fn subcommand_clis_all_have_a_subcommand_list() {
    for cli in SUBCOMMAND_CLIS {
        assert!(known_subcommands(cli).is_some(), "{cli}");
        assert!(cli.len() >= MIN_TYPO_COMMAND_LEN, "{cli}");
    }
}

/// #1980: a typo of a command, on a line shaped like that command's, runs in the shell (which
/// says `command not found`) instead of going to the agent; with the session's answer about the
/// head or without it.
#[test]
fn a_mistyped_command_in_a_conversation_goes_to_the_shell() {
    for line in [
        "gti status",
        "gt status",
        "gitt push origin main",
        "dokcer ps",
        "carg build",
        "kubctl get pods",
        "nmp install",
        "gerp -rn TODO src/",
        "mkdri build/out",
        "gti status # where are we",
    ] {
        for absent in [false, true] {
            let decision = route(
                &unparsed(line),
                &context(Surface::AgentConversation, absent, false),
            );
            assert_eq!(
                (decision.route, decision.rule),
                (Route::Shell, Rule::MistypedCommandInConversation),
                "{line:?} (absent: {absent})"
            );
        }
    }
    // With an assignment in front the command-shape rule may take the line first; either way it
    // is the shell's.
    for absent in [false, true] {
        let decision = route(
            &unparsed("FOO=1 gti status"),
            &context(Surface::AgentConversation, absent, false),
        );
        assert_eq!(decision.route, Route::Shell, "{}", decision.rule);
    }
}

/// A head that is near no built-in command is the shell's only when the caller found it near a
/// command the session knows, and never with English arguments.
#[test]
fn a_typo_of_a_session_command_needs_the_session_to_say_so() {
    let line = "deployx prod-eu1";
    for absent in [false, true] {
        let without = route(
            &unparsed(line),
            &context(Surface::AgentConversation, absent, false),
        );
        assert_ne!(without.route, Route::Shell, "{}", without.rule);
        let with = route(
            &unparsed(line),
            &context(Surface::AgentConversation, absent, true),
        );
        assert_eq!(
            (with.route, with.rule),
            (Route::Shell, Rule::MistypedCommandInConversation)
        );
        let english = route(
            &unparsed("deployx staging now"),
            &context(Surface::AgentConversation, absent, true),
        );
        assert_eq!(english.route, Route::Agent, "{}", english.rule);
    }
}

/// Messages to the agent whose first word happens to be near a command keep going to the agent,
/// lone words are left alone (#935), and the plain terminal does not have the rule.
#[test]
fn prose_with_a_near_miss_head_never_takes_the_typo_rule() {
    for line in [
        "lets go",
        "helo world",
        "tom said so",
        "gut feeling about this",
        "gti is broken again",
        "gti status please",
        "gti status?",
        "gti what now",
        "gti status for me",
        "Gti status",
        "gti",
        "gt",
        "gti\nstatus",
    ] {
        for absent in [false, true] {
            for near_installed in [false, true] {
                let decision = route(
                    &unparsed(line),
                    &context(Surface::AgentConversation, absent, near_installed),
                );
                assert_ne!(
                    decision.rule,
                    Rule::MistypedCommandInConversation,
                    "{line:?} (absent: {absent}, near: {near_installed}): {:?}",
                    decision.route
                );
            }
        }
    }
    for line in ["gti is broken again", "gti status please", "helo world"] {
        let decision = route(
            &unparsed(line),
            &context(Surface::AgentConversation, true, false),
        );
        assert_eq!(decision.route, Route::Agent, "{line:?}: {}", decision.rule);
    }
    for line in ["gti status", "gerp -rn TODO src/"] {
        for absent in [false, true] {
            let decision = route(
                &unparsed(line),
                &context(Surface::PlainTerminal, absent, true),
            );
            assert_ne!(
                decision.rule,
                Rule::MistypedCommandInConversation,
                "{line:?}"
            );
        }
    }
}

/// The head the app looks up in the session's command set: an undescribed typo candidate with
/// something after it, and nothing else.
#[test]
fn the_session_lookup_is_asked_only_for_a_typo_candidate() {
    for (line, expected) in [
        ("kubctl get pods", Some("kubctl")),
        ("  gti status", Some("gti")),
        ("gti", None),
        ("gt status", None),
        ("make the build", None),
        ("what is this", None),
        ("Gti status", None),
        ("./gti status", None),
        ("FOO=1 gti status", None),
        ("", None),
    ] {
        assert_eq!(
            mistyped_head_candidate(&unparsed(line)),
            expected,
            "{line:?}"
        );
    }
    assert!(is_near_a_command("kubctl", ["ls", "kubectl", "git"]));
    assert!(!is_near_a_command("kubctl", ["ls", "git", "kubctl"]));
    assert!(!is_near_a_command("sl", ["ls"]));
    assert!(!is_near_a_command("lsa", ["ls"]));
}

fn powershell(surface: Surface, shell: ShellFamily) -> RoutingContext {
    RoutingContext {
        shell,
        ..context(surface, false, false)
    }
}

/// #2113 (round 14, Windows PowerShell pane): `[IO.File]::AppendAllText('...', '...')` was left
/// to the model (its first word carries a quote), which sent it to the agent or the shell by
/// turns, and a lone `1..300000` went to the agent in a conversation for lack of evidence.
#[test]
fn powershell_type_and_range_heads_are_shell_syntax_in_a_powershell_session() {
    for line in [
        "[IO.File]::AppendAllText('C:\\w\\iw_emoji.txt', '\u{1F600}\u{1F680}' + \"`n\", (New-Object Text.UTF8Encoding $false))",
        "[IO.File]::WriteAllText('C:\\w\\ag_ps.txt', '@\u{20AC}{[]}', (New-Object Text.UTF8Encoding $false))",
        "[IO.File]::AppendAllText('ip_cjk.txt', '\u{65E5}\u{672C}\u{8A9E}')",
        "[math]::Sqrt(2)",
        "[datetime]::Now",
        "[System.Collections.Generic.List[string]]::new()",
        "[int[]]$a = 1, 2, 3",
        "[int]$x = 5",
        "[string]${name}",
        "1..300000",
        "1..10 | ForEach-Object { $_ * 2 }",
        "0..$n",
    ] {
        for surface in [Surface::PlainTerminal, Surface::AgentConversation] {
            let decision = route(
                &unparsed(line),
                &powershell(surface, ShellFamily::PowerShell),
            );
            assert_eq!(
                (decision.route, decision.rule),
                (Route::Shell, Rule::ShellSyntax),
                "{line:?} on {surface:?}"
            );
        }
    }
}

/// The rule is PowerShell's alone, and only for these two shapes: a bracketed word that is no
/// type expression, a decimal and prose keep the rules they had.
#[test]
fn powershell_expression_rule_takes_nothing_else() {
    for line in [
        "[WIP] fix the build",
        "[1]: see the note below",
        "[IO.File] is a type, what does it do",
        "[x] done",
        "[int]5 apples",
        "[a b]::c",
        "[]::x",
        "1.5 is the version",
        "1...",
        "..10",
        "a..b",
        "1..",
        "FOO=1 [math]::PI",
    ] {
        for surface in [Surface::PlainTerminal, Surface::AgentConversation] {
            let decision = route(
                &unparsed(line),
                &powershell(surface, ShellFamily::PowerShell),
            );
            assert_ne!(decision.rule, Rule::ShellSyntax, "{line:?} on {surface:?}");
        }
    }
    for line in ["[math]::Sqrt(2)", "[IO.File]::Exists('x')", "1..10"] {
        for shell in [
            ShellFamily::Posix,
            ShellFamily::Fish,
            ShellFamily::Nu,
            ShellFamily::Unknown,
        ] {
            for surface in [Surface::PlainTerminal, Surface::AgentConversation] {
                let decision = route(&unparsed(line), &powershell(surface, shell));
                assert_ne!(
                    decision.rule,
                    Rule::ShellSyntax,
                    "{line:?} in {shell:?} on {surface:?}"
                );
            }
        }
    }
}
