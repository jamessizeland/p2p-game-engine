//! Peer connection and forfeit handling for room events.

use super::{HostEvent, ui::UiEvent};
use crate::{ConnectionEffect, GameLogic, PeerMap, PeerStatus, room::state::StateData};
use anyhow::Result;
use iroh::EndpointId;
use std::sync::Arc;

/// Process a peer join event from the iroh doc.
pub(super) async fn process_joiner<G: GameLogic>(
    id: EndpointId,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
) -> Option<UiEvent<G>> {
    if state_data.is_host().await.unwrap_or_default() {
        state_data
            .set_peer_status(&id, PeerStatus::Online)
            .await
            .ok();

        if let Ok(mut current_state) = state_data.get_game_state().await {
            let mut players = state_data.get_peer_list().await.unwrap_or_default();
            if players.contains_key(&id)
                && let Ok(effect) =
                    logic.handle_player_reconnect(&mut players, &id, &mut current_state)
            {
                persist_connection_effect(state_data, &players, &current_state, effect)
                    .await
                    .ok();
            }
        }
    } else if state_data.is_peer_host(&id).await.unwrap_or_default() {
        state_data.host_online();
        return Some(UiEvent::Host(HostEvent::Online));
    }
    None
}

/// Process a peer leave event from the iroh doc.
pub(super) async fn process_leaver<G: GameLogic>(
    id: EndpointId,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
) -> Option<UiEvent<G>> {
    if state_data.is_host().await.unwrap_or_default() {
        state_data
            .set_peer_status(&id, PeerStatus::Offline)
            .await
            .ok();

        if let Ok(mut current_state) = state_data.get_game_state().await {
            let mut players = state_data.get_peer_list().await.unwrap_or_default();
            if let Ok(effect) =
                logic.handle_player_disconnect(&mut players, &id, &mut current_state)
            {
                persist_connection_effect(state_data, &players, &current_state, effect)
                    .await
                    .ok();
            }
        }
    } else if state_data.is_peer_host(&id).await.unwrap_or_default() {
        state_data.host_offline();
        return Some(UiEvent::Host(HostEvent::Offline));
    }
    None
}

/// Apply standard forfeit behavior and game-specific forfeit hooks.
pub(super) async fn process_forfeit<G: GameLogic>(
    data: &StateData<G>,
    logic: &Arc<G>,
    node_id: &EndpointId,
) -> Result<()> {
    data.set_peer_observer(node_id, true).await?;
    let mut current_state = match data.get_game_state().await {
        Ok(state) => state,
        Err(_) => return Ok(()),
    };
    let mut players = data.get_peer_list().await.unwrap_or_default();
    if let Some(peer) = players.get_mut(node_id) {
        peer.is_observer = true;
    }
    let effect = logic.handle_player_forfeit(&mut players, node_id, &mut current_state)?;
    persist_connection_effect(data, &players, &current_state, effect).await
}

/// Persist the state and peer changes requested by a connection hook.
async fn persist_connection_effect<G: GameLogic>(
    data: &StateData<G>,
    players: &PeerMap,
    current_state: &G::GameState,
    effect: ConnectionEffect,
) -> Result<()> {
    match effect {
        ConnectionEffect::NoChange => {}
        ConnectionEffect::StateChanged => data.set_game_state(current_state).await?,
        ConnectionEffect::PeersChanged => data.persist_peer_list(players).await?,
        ConnectionEffect::StateAndPeersChanged => {
            data.persist_peer_list(players).await?;
            data.set_game_state(current_state).await?;
        }
    }
    Ok(())
}
