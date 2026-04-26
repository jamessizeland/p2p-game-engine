//! Document entry processing for room events.

use super::{HostEvent, actions::apply_action_request, connections::process_forfeit, ui::UiEvent};
use crate::{
    ActionResult, AppState, GameLogic, PeerProfile, PeerStatus,
    room::{chat::ChatMessage, state::*},
};
use anyhow::{Result, anyhow};
use iroh_docs::sync::Entry;
use std::sync::Arc;

/// Process a single iroh log entry and produce an optional UI event.
pub async fn process_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<UiEvent<G>>> {
    if let Some(event) = process_host_entry(entry, data, logic).await? {
        return Ok(Some(event));
    }
    process_peer_entry(entry, data, logic).await
}

/// Process entries that only the host should mutate in response to.
async fn process_host_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<UiEvent<G>>> {
    if let Some(node_id) = entry.is_join() {
        if !data.is_host().await? {
            return Ok(None);
        }
        let node_id = node_id?;
        let profile = data
            .parse::<PeerProfile>(entry)
            .await
            .map_err(|e| anyhow!("Failed to parse PeerInfo for {}: {e}", &node_id))?;
        data.insert_peer(&node_id, entry.author(), profile).await?;
        return Ok(None);
    }

    if let Some(action_key) = entry.is_action_request() {
        if !data.is_host().await? {
            return Ok(None);
        }
        process_action_entry(entry, data, logic, action_key?).await?;
        return Ok(None);
    }

    Ok(None)
}

/// Process entries that every peer may translate into UI events.
async fn process_peer_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<UiEvent<G>>> {
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
    }

    if entry.is_peer_entry() {
        return match data.get_peer_list().await {
            Err(e) => Err(anyhow!("Failed to get peers list after update: {e}")),
            Ok(peers) => Ok(Some(UiEvent::Peer(peers))),
        };
    }

    if entry.is_game_state_update() {
        if !data.host_author_matches(&entry.author()).await? {
            return Ok(None);
        }
        return match data.parse::<G::GameState>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse GameState: {e}")),
            Ok(state) => Ok(Some(UiEvent::GameState(state))),
        };
    }

    if entry.is_app_state_update() {
        if !data.host_author_matches(&entry.author()).await? {
            return Ok(None);
        }
        return match data.parse::<AppState>(entry).await {
            Err(e) => Err(anyhow!("Failed to parse AppState: {e}")),
            Ok(app_state) => Ok(Some(UiEvent::AppState(app_state))),
        };
    }

    if entry.is_host_update() {
        return process_host_update(entry, data).await;
    }

    if let Some(node_id) = entry.is_quit_request() {
        process_quit_entry(
            data,
            logic,
            node_id?,
            data.parse::<LeaveReason<G>>(entry).await?,
        )
        .await?;
    }

    Ok(None)
}

/// Process an action request entry on the host.
async fn process_action_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
    (node_id, action_id): (iroh::EndpointId, String),
) -> Result<()> {
    if data.has_processed_action(&node_id, &action_id).await? {
        return Ok(());
    }

    if data
        .get_peer_info(&node_id)
        .await?
        .is_some_and(|peer| peer.is_observer)
    {
        let result = ActionResult {
            action_id,
            accepted: false,
            error: Some("Peer is an observer".to_string()),
        };
        data.set_action_result(&node_id, &result).await?;
        data.mark_action_processed(&node_id, &result.action_id)
            .await?;
        return Ok(());
    }

    if !data.peer_author_matches(&node_id, &entry.author()).await? {
        let result = ActionResult {
            action_id,
            accepted: false,
            error: Some("Action author does not match registered peer".to_string()),
        };
        data.set_action_result(&node_id, &result).await?;
        return Ok(());
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
        .await
}

/// Process a host id update.
async fn process_host_update<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
) -> Result<Option<UiEvent<G>>> {
    match data.iroh()?.get_content_bytes(entry).await {
        Err(e) => Err(anyhow!("Failed to parse HostId: {e}")),
        Ok(host_id) => {
            data.host_online();
            let host_id = endpoint_id_from_str(&String::from_utf8_lossy(&host_id))?;
            let peer = data.get_peer_name(&host_id).await?;
            Ok(Some(UiEvent::Host(HostEvent::Changed { to: peer })))
        }
    }
}

/// Process a peer quit or forfeit request.
async fn process_quit_entry<G: GameLogic>(
    data: &StateData<G>,
    logic: &Arc<G>,
    node_id: iroh::EndpointId,
    reason: LeaveReason<G>,
) -> Result<()> {
    if node_id == data.endpoint_id {
        if matches!(reason, LeaveReason::Forfeit) && data.is_host().await.unwrap_or_default() {
            process_forfeit(data, logic, &node_id).await?;
            elect_next_host(data, logic, &node_id).await?;
        }
        return Ok(());
    }

    if data.is_peer_host(&node_id).await.unwrap_or_default() {
        if matches!(reason, LeaveReason::Forfeit) {
            if data.is_host().await.unwrap_or_default() {
                process_forfeit(data, logic, &node_id).await?;
                elect_next_host(data, logic, &node_id).await?;
            }
            return Ok(());
        }
        data.host_offline();
        return Ok(());
    }

    if data.is_host().await.unwrap_or_default() {
        if matches!(reason, LeaveReason::Forfeit) {
            process_forfeit(data, logic, &node_id).await?;
        } else {
            data.set_peer_status(&node_id, PeerStatus::Offline).await?;
        }
    }

    Ok(())
}

/// Elect the next available host after a host forfeit.
async fn elect_next_host<G: GameLogic>(
    data: &StateData<G>,
    logic: &Arc<G>,
    old_host: &iroh::EndpointId,
) -> Result<()> {
    if let Some(new_host) = data.next_host_candidate(logic, Some(old_host)).await? {
        data.set_host(&new_host).await?;
    }
    Ok(())
}
