# Round 10: routing typo, literal `\n` strings, macOS keychain log (#1980, #1842, #1977)

Base: merge/w0 `41340e2fd`. Nothing here was compiled or run; see the report for the compile doubts.

## #1980: a mistyped command in an agent conversation goes to the agent

**Cause.** `crates/input_classifier/src/util/routing.rs:398-411`: in a conversation, a head that is
not installed, not a curated command word, not a keyword and not a path ends in
`Decision::agent(Rule::UnknownHeadInConversation)` unless an argument is data-shaped (then the
classifier decides). `gti status` has evidence by `lacks_command_evidence_in_agent_conversation`
(`util.rs:2453`, the head is no English word), no data-shaped argument, so it is sent to the model.

**Fix.** A new rule, `Rule::MistypedCommandInConversation`, tried inside that unknown-head branch
(after the `please` rule, before the data-shape / agent fall-through). The line goes to the shell,
which prints `command not found`, when all of these hold:

- the head is a typo candidate: 2 to 32 lowercase ASCII letters, digits, `-` or `_`, starting with
  a letter, not an English dictionary word, not a StackOverflow-vocabulary word, not a question
  word, request opener, one-off natural-language word or ambiguous lone head;
- the line has at least one argument, is one line, does not end in `?`, has no prose veto word
  (`has_prose_veto_word`: function words, the article `a`, sentence punctuation, misplaced `re-`
  verbs) and no `STRONG_PROSE_WORDS` member among the arguments;
- and one of:
  1. the head is one edit (one substitution, insertion, deletion or adjacent transposition) from a
     subcommand CLI of three or more letters (`git`, `cargo`, `docker`, `kubectl`, ...) and the
     first argument is one of that CLI's `known_subcommands` (`gti status`, `dokcer ps`,
     `carg build`, `gt status`);
  2. the head has three or more letters and is one edit from a command of three or more letters
     that is either in the curated `UNAMBIGUOUS_SHELL_COMMAND_WORDS` set or in the session's own
     command set (`RoutingContext::head_near_installed_command`), and no argument reads as an
     English word (`is_prose_word`): `gerp -rn TODO src/`, `mkdri build/out`.

The session's command set is supplied by the caller as one boolean. The keystroke path
(`BlocklistAIInputModel::detect_and_set_input_type_with_facts`) computes it from
`CompletionContext::top_level_commands()`, only in an agent view and only when
`routing::mistyped_head_candidate` says the head qualifies (so an installed, described head, a
lone word or an English head never scans the set). The scan is one pass with a length filter and
a byte comparison per name, the same order of work as `Session::knows_command_word`, which the
Enter path already does. It was not measured.

**Enter path gap (needs `app/src/terminal/input.rs`, which this round must not edit).**
`settle_pending_detection_for_submit` gained no parameter, because its one production caller is in
`input.rs` (owned by the #1661 work). At an Enter that beats the parse the rule therefore sees the
static sets only (tier 1 and the curated half of tier 2). `gti status` and the other common CLIs
are covered on both paths; a typo of a command that is only in the session's set (a user alias, an
uncommon tool) is routed to the shell at the keystroke and stays as before (agent or classifier)
at a fast Enter. The follow-up is one argument: pass
`routing::mistyped_head_candidate(&snapshot).is_some_and(|head| routing::is_near_a_command(head, session.top_level_commands()))`
into `settle_pending_detection_for_submit`.

**Rejected.**
- Any unknown head near a command goes to the shell regardless of arguments: `lets go`, `helo
  world`, `tom said so` are one edit from `less`, `help`, `top`. English arguments keep the line
  with the agent unless the first one is a subcommand of the near CLI.
- Lone words (`gti`, `pdw`): a single unknown word in a conversation is as often an answer to the
  agent (a name, a branch, a crate) as a typo; #935 / #1290 pin lone unknown words to the agent.
  Left as is.
- The plain terminal: untouched (an unknown head there is already the classifier's, #833).
- Requiring the near command to be installed for tier 1: the outcome is `command not found` either
  way, and the evidence is the shape (near a CLI plus that CLI's own subcommand), not the install.
- A corpus row in `routing_corpus.rs`: those rows also run against the pre-#1234 procedure with
  strict xfail, which cannot be judged without running it. The cases are direct tests of `route`.

**Risk.** A conversation message whose first word is a non-English token one edit from a command
and whose other words are all non-English data (`tokio 1.38`) now runs in the shell and prints
`command not found`; before, the classifier decided it. One click on the misroute recovery (#835)
sends it to the agent. #1560 (shell structure wins) runs earlier and is unchanged; the
`routing_table` lines do not have a near-miss head with a matching shape (`notacmd_x`,
`xyz_nocmd`, `foo_nocmd` contain `_` plus letters and are not within one edit of any command).

## #1842: UI strings show a literal `\n`

**Cause.** Fluent has no backslash escapes in pattern text, so `\n` in a `.ftl` value is two
characters. Remaining values (en, ja, zh-CN `warp.ftl`): `launch-config-yaml-saved-to-prefix`
(`app/src/launch_configs/save_modal.rs:597`), `onboarding-callout-updated-agent-input-project-text`
(`crates/onboarding/src/callout/view.rs:260`), `auth-browser-sign-in-title` (no caller).

**Fix.** The two paragraph strings become Fluent multiline values (a blank line inside a block
pattern is kept; `settings-show-blocks-confirm-dialog-text` already relies on that). The save
modal's leading line break moves into the code (`format!("\n{}", t!(..))`), which is what the pin
has (`"\nThe YAML file is saved to "`). A guard, `script/check_ftl_escapes`, fails on `\n`, `\t` or
`\r` in any `.ftl` line that is not a comment; `{"\\n"}` is the way to show a real backslash-n. It
runs in `script/precheck` and the CI gates job.

**Rejected.** `{"\u000A"}` placeables (valid, but unreadable for translators); unescaping in `t!`
(would change every string that legitimately shows a backslash).

**Risk.** Layout only. Fluent strips the common indent of block text; the lines do not start with
`[`, `*`, `.` or `}`.

## #1977: macOS relaunch logs ERROR "A default keychain could not be found" twice

**Verdict: mostly a test-environment fact, plus a small app defect.**

- The r18w8 `agent_restart_kill_*` FAILs are from a harness without commit `2f1af0f88`
  (`judge_errors` drops these two lines on macOS; `git merge-base --is-ancestor 2f1af0f88 0b817b355`
  is false). The Mac test session is an ssh session with no login keychain, so every launch there
  logs them, the first launch included; it is not specific to a relaunch.
- "Credential lookup on the first-frame path" is not what the code does: both readers run on the
  secure-storage worker (`app/src/settings/local_control.rs:166` `awaiting_load`, #1309;
  `crates/ai/src/api_keys.rs:296` `read_keys` from the background load, #1805).
- The app defect: `crates/warpui_extras/src/secure_storage/mac.rs:51` turns a failing
  `SecKeychain::default()` into `Error::Unknown`, which every reader logs at ERROR
  (`local_control.rs:88`, `api_keys.rs:302`), once per reader per launch, and the user is told
  nothing. Linux already has the right shape (#1805): the outage is recorded once in
  `secure_storage::availability`, reads answer `NotFound`, and `keyring_notice.rs` shows one
  non-modal toast.

**Fix.** `mac.rs` records a default keychain that cannot be opened in `availability`
(`mark_unavailable`, one WARN line per run), reads and removals answer `NotFound` like the Linux
backend, writes still return the error (nothing was stored, the caller must know). A keychain that
opens marks the state available again, so the toast's retry works. The existing
`KeyringNotice` then shows the one-line notice on macOS too.

**Rejected.** Dropping the readers' ERROR lines (they are right for a real read failure);
probing the keychain at startup (a lookup on the launch path is what the issue asks to avoid).

**Risk.** On the Mac test box every launch now shows the 10 s keyring toast (as Linux did before
its launcher seeded a keyring). A case that counts toasts or compares screenshots in the first
10 s may notice; the harness filter `ENV_ERR_RE` stays as it is.
