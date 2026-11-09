//! The fake model behind the agent fuzzer (docs/testing.md, "Agent fuzzer and fake model").
//!
//! * [`scenario`]: the `"v": 1` JSON schema every layer shares (scripted turns, tool calls in all
//!   their malformed shapes, command behaviours, chaos, the scripted user).
//! * [`llm`]: scenario -> llmposter fixtures (HTTP mock, SSE, status codes, truncation).
//! * [`wire`]: what llmposter cannot do (byte-level cuts, split tool-call arguments, a scripted
//!   raw server, a chaos proxy).
//! * [`strategies`]: proptest strategies over the schema.
//! * [`decode`]: a real genai client pointed at a base URL, for the L0 wire tests.
//! * [`aws_stub`]: a tiny HTTP stub for AWS-shaped endpoints (STS, SSO, OIDC, IMDS, sign-in, and a
//!   Bedrock `Converse` endpoint) with an independent SigV4 verifier, for the credential resolver tests.
//!
//! Test-only: a dev-dependency of `app`, never linked into a release build.

pub mod aws_stub;
pub mod decode;
pub mod llm;
pub mod scenario;
pub mod strategies;
pub mod wire;

#[cfg(test)]
mod genai_decode_tests;
