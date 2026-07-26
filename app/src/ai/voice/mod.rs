//! This module contains all code relevant to Voice within Zap.
//!
//! Voice is used for voice input within Zap.

// Zap Wave 6-1: `pub(crate) mod transcribe` was physically deleted along with
// `ServerApi::transcribe`. The atomic module `transcribe/api/{request,response}` was
// only the wire types for the now-deleted cloud `/ai/transcribe` endpoint. Local
// voice goes through the `voice/transcriber.rs::Transcriber` trait + `TranscribeError`.
