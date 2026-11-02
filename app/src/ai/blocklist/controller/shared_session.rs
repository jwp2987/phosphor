// Local display logic for BlocklistAIController's shared sessions.
use super::BlocklistAIController;
use crate::terminal::shared_session::ParticipantId;

#[derive(Default)]
pub(super) struct SharedSessionState {
    // The participant who initiated the current response stream
    current_response_initiator: Option<ParticipantId>,
    // The sharer's participant ID (set when session sharing starts)
    sharer_participant_id: Option<ParticipantId>,
}

impl BlocklistAIController {
    /// Sets the participant ID for the current response.
    /// This should be called when initiating a query to track who sent it.
    pub fn set_current_response_initiator(&mut self, participant_id: ParticipantId) {
        self.shared_session_state.current_response_initiator = Some(participant_id);
    }

    /// Gets the participant ID for the current response.
    pub(super) fn get_current_response_initiator(&self) -> Option<ParticipantId> {
        self.shared_session_state.current_response_initiator.clone()
    }

    /// Sets the sharer's participant ID. Should be called when a shared session is created.
    pub fn set_sharer_participant_id(&mut self, participant_id: ParticipantId) {
        self.shared_session_state.sharer_participant_id = Some(participant_id);
    }

    /// Gets the sharer's participant ID.
    pub(super) fn get_sharer_participant_id(&self) -> Option<ParticipantId> {
        self.shared_session_state.sharer_participant_id.clone()
    }
}
