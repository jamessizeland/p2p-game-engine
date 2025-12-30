use crate::{
    AppState, GameLogic, GameRoom, PeerMap, PeerProfile, PeerStatus,
    room::{chat::ChatMessage, state::*},
};
use anyhow::{Result, anyhow};
use iroh::EndpointId;
use iroh_blobs::Hash;
use iroh_docs::{
    ContentStatus,
    engine::{LiveEvent, SyncEvent},
    sync::Entry,
};
use n0_future::StreamExt as _;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Public events your library will send to the game UI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent<G: GameLogic> {
    Peer(PeerMap),
    GameState(G::GameState),
    AppState(AppState),
    Chat { sender: String, msg: ChatMessage },
    Host(HostEvent),
    Error(String), // TODO replace with AppError including G::GameError
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    /// Host has connected
    Online,
    /// Host has disconnected
    Offline,
    /// A new host has been assigned
    Changed { to: String },
}

impl<G: GameLogic> Display for UiEvent<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiEvent::Peer(peers) => write!(f, "PeerUpdated({peers})"),
            UiEvent::GameState(state) => write!(f, "GameStateUpdated({state:?})"),
            UiEvent::AppState(state) => write!(f, "AppStateChanged({state:?})"),
            UiEvent::Chat { sender: _, msg } => write!(f, "Chat({msg:?})"),
            UiEvent::Host(HostEvent::Changed { to }) => write!(f, "HostSet({to})"),
            UiEvent::Host(HostEvent::Offline) => write!(f, "HostOffline"),
            UiEvent::Host(HostEvent::Online) => write!(f, "HostOnline"),
            UiEvent::Error(msg) => write!(f, "Error({msg})"),
        }
    }
}

impl<G: GameLogic> GameRoom<G> {
    pub(crate) async fn start_event_loop(
        &mut self,
    ) -> Result<(mpsc::Receiver<UiEvent<G>>, JoinHandle<()>)> {
        let mut sub = self.state.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let state_data = self.state.clone();
        let logic = self.logic.clone();

        let task_handle = tokio::spawn(async move {
            let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();
            loop {
                tokio::select! {
                    // Listen for iroh doc events
                    Some(Ok(event)) = sub.next() => {
                        let network_event = match NetworkEvent::parse(event, &mut pending_entries)  {
                            Some(event) => event,
                            None => continue,
                        };
                        // println!("{} Network event: {network_event:?}", if state_data.is_host().await.unwrap_or(false) {
                        //         "Host"
                        //     } else {
                        //         "Client"
                        //     });
                        match network_event {
                            NetworkEvent::Update(entry) => match process_entry(&entry, &state_data, &logic).await {
                                Err(e) => eprintln!("Error processing event: {e}"),
                                Ok(None) => {} // No event to send
                                Ok(Some(event)) => {
                                    // Send the event to the UI
                                    // println!("{} UI event: {event}", if state_data.is_host().await.unwrap_or(false) {
                                    //     "Host"
                                    // } else {
                                    //     "Client"
                                    // });
                                    if sender.send(event).await.is_err() {
                                        break; // Channel closed
                                    }
                                }
                            },
                            NetworkEvent::Joiner(id) => {
                                // println!("Joiner: {id}");
                                // A peer has connected, if we are the host we can set its status to online
                                // if they are in our peer list already
                                if state_data.is_host().await.unwrap_or(false) {
                                    // println!("Host is updating status for {id} to Online");
                                    state_data.set_peer_status(&id, PeerStatus::Online).await.ok();
                                } else if state_data.is_peer_host(&id).await.unwrap_or(false) {
                                    // If we are a client, we only care if the peer that joined was the host.
                                    // println!("Client detected host reconnection.");
                                    state_data.host_online();
                                    if sender.send(UiEvent::Host(HostEvent::Online)).await.is_err() {
                                            break; // Channel closed
                                        }
                                }
                            },
                            NetworkEvent::Leaver(id) => {
                                // A peer has disconnected from us.
                                // If we are the host, we are responsible for updating the peer's status.
                                if state_data.is_host().await.unwrap_or(false) {
                                    // println!("Host is updating status for {id} to Offline");
                                    state_data.set_peer_status(&id, PeerStatus::Offline).await.ok();
                                } else if state_data.is_peer_host(&id).await.unwrap_or(false) {
                                        // If we are a client, we only care if the peer that dropped was the host.
                                        println!("Client detected host disconnection.");
                                        state_data.host_offline();
                                        if sender.send(UiEvent::Host(HostEvent::Offline)).await.is_err() {
                                            break; // Channel closed
                                        }
                                }
                            },
                            NetworkEvent::SyncFailed(reason) => {
                                let error = UiEvent::Error(format!("Sync failed: {reason}"));
                                // eprintln!("Error processing event: {error}");
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
        });
        Ok((receiver, task_handle))
    }
}

async fn process_entry<G: GameLogic>(
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
        let profile = match data.parse::<PeerProfile>(&entry).await {
            Ok(profile) => profile,
            Err(e) => {
                return Err(anyhow!("Failed to parse PeerInfo for {}: {e}", &node_id,));
            }
        };
        // Broadcast the new canonical peer list
        data.insert_peer(&node_id, profile).await?;
        // The `insert_peer` will trigger a `peer_entry` live event, which will
        // in turn trigger the `Peer` ui event. So we don't need to return anything here.
        return Ok(None);
    } else if let Some(node_id) = entry.is_action_request() {
        if !data.is_host().await? {
            return Ok(None);
        }
        let node_id = node_id?;
        // Ensure we have a state to apply the action to
        let current_state = &mut data.get_game_state().await?;

        match data.parse::<G::GameAction>(&entry).await {
            Ok(action) => {
                // Apply the game logic and broadcast the new authoritative state
                match logic.apply_action(current_state, &node_id, &action) {
                    Err(e) => {
                        let peer = data.get_peer_name(&node_id).await?;
                        return Err(anyhow!("Invalid action from {peer}: {e}"));
                    }
                    Ok(()) => data.set_game_state(current_state).await?,
                };
            }
            Err(e) => {
                let peer = data.get_peer_name(&node_id).await?;
                return Err(anyhow!("Failed to parse GameAction from {peer}: {e}",));
            }
        }
        // The `set_game_state` will trigger a `game_state_update` live event, which will
        // in turn trigger the `GameState` ui event. So we don't need to return anything here.
        return Ok(None);
    }
    // --- ALL-PEERS LOGIC ---
    if let Some(node_id) = entry.is_chat_message() {
        let node_id = node_id?;
        let sender = data.get_peer_name(&node_id).await?;
        return match data.parse::<ChatMessage>(&entry).await {
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
        // The game state has been updated by the host.
        return match data.parse::<G::GameState>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse GameState: {e}")),
            Ok(state) => Ok(Some(UiEvent::GameState(state))),
        };
    } else if entry.is_app_state_update() {
        // The app state has been updated by the host.
        return match data.parse::<AppState>(&entry).await {
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
        } else {
            return Ok(None); // TODO handle preparing leaver
        }
    }
    // println!("unknown event: {entry:?}");
    Ok(None)
}

#[derive(Debug)]
enum NetworkEvent {
    Update(Entry),
    Joiner(EndpointId),
    Leaver(EndpointId),
    SyncFailed(String),
    SyncSucceeded,
}

impl NetworkEvent {
    /// Output a doc entry when a new one is ready.
    fn parse(event: LiveEvent, pending_entries: &mut HashMap<Hash, Entry>) -> Option<Self> {
        use ContentStatus::{Complete, Incomplete, Missing};
        match event {
            LiveEvent::InsertLocal { entry } => Some(Self::Update(entry)),
            LiveEvent::InsertRemote {
                entry,
                content_status: Complete,
                ..
            } => Some(Self::Update(entry)),
            LiveEvent::InsertRemote {
                entry,
                content_status: Missing | Incomplete,
                ..
            } => {
                pending_entries.insert(entry.content_hash(), entry);
                None
            }
            LiveEvent::ContentReady { hash } => pending_entries
                .remove(&hash)
                .map(|entry| Self::Update(entry)),
            LiveEvent::NeighborUp(id) => Some(Self::Joiner(id)),
            LiveEvent::NeighborDown(id) => Some(Self::Leaver(id)),
            LiveEvent::SyncFinished(SyncEvent { result, .. }) => match result {
                Ok(_) => Some(Self::SyncSucceeded),
                Err(reason) => Some(Self::SyncFailed(reason)),
            },
            _other => None,
        }
    }
}
