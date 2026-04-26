use super::{
    network::NetworkEvent,
    process::process_entry,
    ui::{UiError, UiEvent},
};
use crate::{ConnectionEffect, GameLogic, GameRoom, PeerMap, PeerStatus, room::state::StateData};
use anyhow::Result;

use iroh_blobs::Hash;
use iroh_docs::{Entry, engine::LiveEvent};
use n0_future::{Stream, StreamExt as _};
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Public events your library will send to the game UI

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    /// Host has connected
    Online,
    /// Host has disconnected
    Offline,
    /// A new host has been assigned
    Changed { to: String },
}

impl<G: GameLogic> GameRoom<G> {
    pub(crate) async fn start_event_loop(
        &mut self,
    ) -> Result<(mpsc::Receiver<UiEvent<G>>, JoinHandle<()>)> {
        let sub = self.state.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let state_data = self.state.clone();
        let logic = self.logic.clone();

        let task_handle = tokio::spawn(async move {
            event_loop(sub, sender, state_data, &logic).await;
        });
        Ok((receiver, task_handle))
    }
}

/// Main event loop that listens for iroh doc events and processes them.
async fn event_loop<G: GameLogic>(
    mut sub: impl Stream<Item = Result<LiveEvent>> + Unpin,
    sender: mpsc::Sender<UiEvent<G>>,
    state_data: Arc<StateData<G>>,
    logic: &Arc<G>,
) {
    let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();
    loop {
        tokio::select! {
            // Listen for iroh doc events
            Some(Ok(event)) = sub.next() => {
                let network_event = match NetworkEvent::parse(event, &mut pending_entries)  {
                    Some(event) => event,
                    None => continue,
                };
                match network_event {
                    NetworkEvent::Update(entry) => if process_update(&entry, &state_data, logic, &sender).await.is_closed()  {
                        break;
                    },
                    NetworkEvent::Joiner(id) => if process_joiner(id, &state_data, logic, &sender).await.is_closed() {
                        break;
                    },
                    NetworkEvent::Leaver(id) => if process_leaver(id, &state_data, logic, &sender).await.is_closed() {
                        break;
                    },
                    NetworkEvent::SyncFailed(reason) => {
                        let error = UiEvent::Error(UiError::SyncFailed(reason));
                        if sender.send(error).await.is_err() {
                                break; // Channel closed
                            }
                        },
                    NetworkEvent::SyncSucceeded => { /* Do nothing for now */},
                }
            },
            else => break, // Stream finished
        }
    }
}

enum ChannelStatus {
    Open,
    Closed,
}

impl ChannelStatus {
    fn is_closed(&self) -> bool {
        matches!(self, ChannelStatus::Closed)
    }
}

/// Process an update event from the iroh doc.
async fn process_update<G: GameLogic>(
    entry: &Entry,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
    sender: &mpsc::Sender<UiEvent<G>>,
) -> ChannelStatus {
    match process_entry(&entry, &state_data, &logic).await {
        Ok(None) => {} // No event to send
        Ok(Some(event)) => {
            // Send the event to the UI
            if sender.send(event).await.is_err() {
                return ChannelStatus::Closed;
            }
        }
        Err(e) => {
            let error = UiEvent::Error(UiError::EventProcessing {
                key: String::from_utf8_lossy(entry.key()).to_string(),
                author: entry.author().to_string(),
                message: e.to_string(),
            });
            if sender.send(error).await.is_err() {
                return ChannelStatus::Closed;
            }
        }
    }
    ChannelStatus::Open // Continue the loop
}

/// Process a peer join event from the iroh doc.
async fn process_joiner<G: GameLogic>(
    id: iroh::PublicKey,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
    sender: &mpsc::Sender<UiEvent<G>>,
) -> ChannelStatus {
    // A peer has connected, if we are the host we can set its status to online
    // if they are in our peer list already
    if state_data.is_host().await.unwrap_or(false) {
        state_data
            .set_peer_status(&id, PeerStatus::Online)
            .await
            .ok();

        // Trigger GameLogic hook
        if let Ok(mut current_state) = state_data.get_game_state().await {
            let mut players = state_data.get_peer_list().await.unwrap_or_default();
            if players.contains_key(&id) {
                if let Ok(effect) =
                    logic.handle_player_reconnect(&mut players, &id, &mut current_state)
                {
                    persist_connection_effect(&state_data, &players, &current_state, effect)
                        .await
                        .ok();
                }
            }
        }
    } else if state_data.is_peer_host(&id).await.unwrap_or(false) {
        // If we are a client, we only care if the peer that joined was the host.
        state_data.host_online();
        if sender.send(UiEvent::Host(HostEvent::Online)).await.is_err() {
            return ChannelStatus::Closed;
        }
    }
    ChannelStatus::Open
}

/// Process a peer leave event from the iroh doc.
async fn process_leaver<G: GameLogic>(
    id: iroh::PublicKey,
    state_data: &Arc<StateData<G>>,
    logic: &Arc<G>,
    sender: &mpsc::Sender<UiEvent<G>>,
) -> ChannelStatus {
    // A peer has disconnected from us.
    // If we are the host, we are responsible for updating the peer's status.
    if state_data.is_host().await.unwrap_or(false) {
        state_data
            .set_peer_status(&id, PeerStatus::Offline)
            .await
            .ok();

        // Trigger GameLogic hook
        if let Ok(mut current_state) = state_data.get_game_state().await {
            let mut players = state_data.get_peer_list().await.unwrap_or_default();
            if let Ok(effect) =
                logic.handle_player_disconnect(&mut players, &id, &mut current_state)
            {
                persist_connection_effect(&state_data, &players, &current_state, effect)
                    .await
                    .ok();
            }
        }
    } else if state_data.is_peer_host(&id).await.unwrap_or(false) {
        // If we are a client, we only care if the peer that dropped was the host.
        state_data.host_offline();
        if sender
            .send(UiEvent::Host(HostEvent::Offline))
            .await
            .is_err()
        {
            return ChannelStatus::Closed;
        }
    }
    ChannelStatus::Open
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
