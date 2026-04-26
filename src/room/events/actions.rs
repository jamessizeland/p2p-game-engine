//! Action request handling for room events.

use crate::{
    ActionResult, GameLogic,
    room::state::{ActionRequest, StateData},
};
use anyhow::Result;
use iroh::EndpointId;
use std::sync::Arc;

/// Apply a parsed action request and produce an accept/reject result.
pub(super) async fn apply_action_request<G: GameLogic>(
    data: &StateData<G>,
    logic: &Arc<G>,
    node_id: &EndpointId,
    request: ActionRequest<G::GameAction>,
) -> Result<ActionResult> {
    let action_id = request.id;
    let mut current_state = match data.get_game_state().await {
        Ok(state) => state,
        Err(e) => {
            return Ok(ActionResult {
                action_id,
                accepted: false,
                error: Some(format!("No game state available: {e}")),
            });
        }
    };

    match logic.apply_action(&mut current_state, node_id, &request.action) {
        Err(e) => Ok(ActionResult {
            action_id,
            accepted: false,
            error: Some(e.to_string()),
        }),
        Ok(()) => {
            data.set_game_state(&current_state).await?;
            Ok(ActionResult {
                action_id,
                accepted: true,
                error: None,
            })
        }
    }
}
