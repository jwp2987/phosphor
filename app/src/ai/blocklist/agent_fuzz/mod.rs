//! The agent fuzzer, layer L1 (docs/testing.md, "Agent fuzzer and fake model").
//!
//! Real terminal view, controller, action model and queue in `App::test`; the model is a script
//! (`fake_llm::scenario`) played through `agent_providers::fake_stream_seam`; a scripted user
//! steps through the same entry points the UI uses; an oracle checks invariants I1-I8 after every
//! step and at the end. A failing case prints its scenario as schema JSON.
//!
//! * `player` - scenario -> seam turns, and the record of what was emitted (I4)
//! * `rig` - app setup, observation, command completion, waiting
//! * `actor` - the scripted user
//! * `oracle` - the invariants
//! * `runner` - one case end to end
//! * `log_capture`, `env` - ERROR-log tap (I6), scripted command behaviours
//! * `coverage` - which (tool x op x timing) and (chaos x phase) cells the run hit
//! * `matrix` (the deterministic approvals matrix: action kind x response, timings, orders, malformed args, command behaviours)
//! * `fuzz_tests` (proptest), `corpus_replay_tests` (+ `corpus/*.json`, `known_failures.toml`),
//!   `prompt_tests` (I5), `stop_tests` (Stop means stop, #1942, counted on the seam's requests)

mod actor;
mod coverage;
mod env;
mod log_capture;
mod oracle;
mod player;
mod rig;
mod runner;

#[cfg(test)]
mod allowlist_tests;
#[cfg(test)]
mod corpus_replay_tests;
#[cfg(test)]
mod fuzz_tests;
#[cfg(test)]
mod matrix;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod smoke_tests;
#[cfg(test)]
mod stop_tests;
