use super::{HostEvent, ui::UiEvent};
use crate::{
    AppState, GameLogic, PeerProfile,
    room::{chat::ChatMessage, state::*},
};

use anyhow::{Result, anyhow};
use iroh::EndpointId;
use iroh_docs::sync::Entry;
use std::sync::Arc;

pub async fn process_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<UiEvent<G>>> {
    // --- HOST LOGIC ---
    if let Some(node_id) = entry.is_join() {
        if !data.is_host().await? {
            return Ok(None);
        }
        let node_id = node_id?;
        // A peer has joined the game room.
        // Get the PeerProfile payload
        let profile = match data.parse::<PeerProfile>(entry).await {
            Ok(profile) => profile,
            Err(e) => {
                return Err(anyhow!("Failed to parse PeerInfo for {}: {e}", &node_id,));
            }
        };
        // Broadcast the new canonical peer list
        data.insert_peer(&node_id, entry.author(), profile).await?;
        // The `insert_peer` will trigger a `peer_entry` live event, which will
        // in turn trigger the `Peer` ui event. So we don't need to return anything here.
        return Ok(None);
    } else if let Some(action_key) = entry.is_action_request() {
        if !data.is_host().await? {
            return Ok(None);
        }
        let (node_id, action_id) = action_key?;
        if data.has_processed_action(&node_id, &action_id).await? {
            return Ok(None);
        }
        if !data.peer_author_matches(&node_id, &entry.author()).await? {
            let result = ActionResult {
                action_id,
                accepted: false,
                error: Some("Action author does not match registered peer".to_string()),
            };
            data.set_action_result(&node_id, &result).await?;
            return Ok(None);
        }

        let result = match data.parse::<ActionRequest<G::GameAction>>(entry).await {
            Ok(request) if request.id == action_id => {
                apply_action_request(data, logic, &node_id, request).await?
            }
            Ok(_) => ActionResult {
                action_id,
                accepted: false,
                error: Some("Action id did not match action key".to_string()),
            },
            Err(e) => ActionResult {
                action_id,
                accepted: false,
                error: Some(format!("Failed to parse action: {e}")),
            },
        };
        data.set_action_result(&node_id, &result).await?;
        data.mark_action_processed(&node_id, &result.action_id)
            .await?;
        if node_id == data.endpoint_id {
            return Ok(Some(UiEvent::ActionResult(result)));
        }
        // The `set_game_state` will trigger a `game_state_update` live event, which will
        // in turn trigger the `GameState` ui event. So we don't need to return anything here.
        return Ok(None);
    }
    // --- ALL-PEERS LOGIC ---
    if let Some(action_result_key) = entry.is_action_result() {
        let (node_id, _action_id) = action_result_key?;
        if node_id != data.endpoint_id {
            return Ok(None);
        }
        return match data.parse::<ActionResult>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse ActionResult: {e}")),
            Ok(result) => Ok(Some(UiEvent::ActionResult(result))),
        };
    }
    if let Some(node_id) = entry.is_chat_message() {
        let node_id = node_id?;
        let sender = data.get_peer_name(&node_id).await?;
        return match data.parse::<ChatMessage>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse ChatMessage from {sender}: {e}")),
            Ok(msg) => Ok(Some(UiEvent::Chat { sender, msg })),
        };
    } else if entry.is_peer_entry() {
        // A peer entry has been added/updated. Fetch the whole list to signal an update.
        return match data.get_peer_list().await {
            Err(e) => Err(anyhow!("Failed to get peers list after update: {e}")),
            Ok(peers) => Ok(Some(UiEvent::Peer(peers))),
        };
    } else if entry.is_game_state_update() {
        if !data.host_author_matches(&entry.author()).await? {
            return Ok(None);
        }
        // The game state has been updated by the host.
        return match data.parse::<G::GameState>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse GameState: {e}")),
            Ok(state) => Ok(Some(UiEvent::GameState(state))),
        };
    } else if entry.is_app_state_update() {
        if !data.host_author_matches(&entry.author()).await? {
            return Ok(None);
        }
        // The app state has been updated by the host.
        return match data.parse::<AppState>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse AppState: {e}")),
            Ok(app_state) => Ok(Some(UiEvent::AppState(app_state))),
        };
    } else if entry.is_host_update() {
        // The host has been claimed/reasigned.
        return match data.iroh()?.get_content_bytes(entry).await {
            Err(e) => Err(anyhow!("Failed to parse HostId: {e}")),
            Ok(host_id) => {
                data.host_online(); // the host has come back online or been claimed.
                let host_id = endpoint_id_from_str(&String::from_utf8_lossy(&host_id))?;
                let peer = data.get_peer_name(&host_id).await?;
                Ok(Some(UiEvent::Host(HostEvent::Changed { to: peer })))
            }
        };
    } else if let Some(node_id) = entry.is_quit_request() {
        let node_id = node_id?;
        // If we are processing our own quit request, do nothing.
        // Let other peers handle it.
        if node_id == data.endpoint_id {
            return Ok(None);
        } else if data.is_peer_host(&node_id).await.unwrap_or(false) {
            data.host_offline();
            return Ok(Some(UiEvent::Host(HostEvent::Offline)));
        } else {
            return Ok(None); // TODO handle preparing leaver
        }
    }
    // println!("unknown event: {entry:?}");
    Ok(None)
}

/// Apply a parsed action request and produce an accept/reject result.
async fn apply_action_request<G: GameLogic>(
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
