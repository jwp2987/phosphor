//! What the scripted commands do (the scenario's `env.commands`). The mock terminal has no pty,
//! so a command never runs; the harness decides when its action finishes from the behaviour.

use fake_llm::scenario::CmdBehaviour;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdOutcome {
    /// Finishes by itself with this output and exit code.
    Completes { output: String, exit_code: i32 },
    /// Keeps running until the user (or a Stop) ends it. The harness reports it to the agent as a
    /// stalled long-running command after a short wait, as the executor's stall snapshot does.
    Stays,
}

pub fn outcome(behaviour: &CmdBehaviour) -> CmdOutcome {
    match behaviour {
        CmdBehaviour::Quick { out } => CmdOutcome::Completes {
            output: out.clone(),
            exit_code: 0,
        },
        // `set -e` and a failing command: the command ends, badly.
        CmdBehaviour::SetE => CmdOutcome::Completes {
            output: String::new(),
            exit_code: 1,
        },
        CmdBehaviour::Silent { .. }
        | CmdBehaviour::TrailingColon { .. }
        | CmdBehaviour::TrailingQuestion { .. }
        | CmdBehaviour::RealPrompt { .. }
        | CmdBehaviour::NeverEnds
        | CmdBehaviour::AltScreen
        | CmdBehaviour::Pager => CmdOutcome::Stays,
    }
}

/// The scenario command key a `run_shell_command` names (`Args::Command` renders
/// `fake-cmd <key>`), if any.
pub fn command_key(command: &str) -> Option<&str> {
    command.strip_prefix("fake-cmd ").map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_quick_and_set_e_finish_by_themselves() {
        assert!(matches!(
            outcome(&CmdBehaviour::Quick { out: "x".into() }),
            CmdOutcome::Completes { exit_code: 0, .. }
        ));
        assert!(matches!(
            outcome(&CmdBehaviour::SetE),
            CmdOutcome::Completes { exit_code: 1, .. }
        ));
        for b in [
            CmdBehaviour::NeverEnds,
            CmdBehaviour::Pager,
            CmdBehaviour::AltScreen,
            CmdBehaviour::Silent { s: String::new() },
            CmdBehaviour::RealPrompt {
                prompt: "p".into(),
                secret: false,
            },
        ] {
            assert_eq!(outcome(&b), CmdOutcome::Stays, "{b:?}");
        }
    }

    #[test]
    fn command_keys_round_trip() {
        assert_eq!(command_key("fake-cmd c1"), Some("c1"));
        assert_eq!(command_key("echo hi"), None);
    }
}
