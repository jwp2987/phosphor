use super::voice_transcription_available;

#[test]
fn voice_transcription_is_not_available_without_a_transcriber_impl() {
    // No `impl Transcriber for` exists anywhere in this tree (the cloud
    // Wispr backend was dropped, see DECLINED.md "Voice input"), so the
    // settings UI that gates on this predicate must stay hidden.
    assert!(!voice_transcription_available());
}
