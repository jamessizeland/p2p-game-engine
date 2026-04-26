//! Facade for translating network events into UI events.

use super::{
    connections,
    entries::process_entry,
    ui::{UiError, UiEvent},
};
use crate::{GameLogic, room::state::StateData};
use iroh::EndpointId;
use iroh_docs::sync::Entry;
use std::sync::Arc;

/// Process an update event from the iroh doc.
pub(super) async fn process_update<G: GameLogic>(
    entry: &Entry,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
) -> Option<UiEvent<G>> {
    match process_entry(entry, state_data, logic).await {
        Ok(maybe_event) => maybe_event,
        Err(e) => Some(UiEvent::Error(UiError::EventProcessing {
            key: String::from_utf8_lossy(entry.key()).to_string(),
            author: entry.author().to_string(),
            message: e.to_string(),
        })),
    }
}

/// Process a peer connection event.
pub(super) async fn process_joiner<G: GameLogic>(
    id: EndpointId,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
) -> Option<UiEvent<G>> {
    connections::process_joiner(id, state_data, logic).await
}

/// Process a peer disconnection event.
pub(super) async fn process_leaver<G: GameLogic>(
    id: EndpointId,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
) -> Option<UiEvent<G>> {
    connections::process_leaver(id, state_data, logic).await
}
