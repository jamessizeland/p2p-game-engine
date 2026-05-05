//! Game Room
//!
//! This module contains the `GameRoom` struct, which is the main interface for creating and joining game rooms,
//! as well as the main API for interacting with the game state. It also contains submodules for handling chat messages,
//! processing events, and querying the state.
//!
//! The `GameRoom` struct is responsible for managing the game state, processing events, and providing an API for the
//! UI to interact with the game.

mod chat;
mod ticket;
mod events {
    mod actions;
    mod connections;
    mod entries;
    mod event_loop;
    mod network;
    mod process;
    mod ui;
    pub use {
        event_loop::HostEvent,
        ui::{UiError, UiEvent},
    };
}
mod snapshot;
mod state;

use crate::{GameLogic, PeerMap, PeerProfile};
use anyhow::Result;
use iroh::EndpointId;
use state::StateData;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, str::FromStr as _};
use tokio::sync::mpsc;

pub use chat::ChatMessage;
pub use events::{HostEvent, UiError, UiEvent};
pub use snapshot::RoomSnapshot;
pub use state::{ActionResult, AppState, LeaveReason};
pub use ticket::GameTicket;

/// The main interface for creating and joining game rooms,
/// as well as the main API for interacting with the game state.
pub struct GameRoom<G: GameLogic> {
    /// Persistent data store
    pub(self) state: Arc<StateData<G>>,
    /// Game logic
    pub(self) logic: Arc<G>,
    /// UI event loop handle
    pub(self) event_handle: Option<tokio::task::JoinHandle<()>>,
    /// The name of the game room created by the host, used for display purposes.
    pub name: String,
}

impl<G: GameLogic> Drop for GameRoom<G> {
    fn drop(&mut self) {
        if let Some(handle) = self.event_handle.take() {
            handle.abort();
        }
    }
}

impl<G: GameLogic> GameRoom<G> {
    fn new(state: StateData<G>, logic: G, name: &str) -> Self {
        Self {
            state: Arc::new(state),
            logic: Arc::new(logic),
            event_handle: None,
            name: name.to_string(),
        }
    }

    /// Get Iroh Network Endpoint ID
    pub fn id(&self) -> EndpointId {
        self.state.endpoint_id
    }
    /// Get a fresh join ticket for this room, including all known peer addresses.
    pub async fn ticket(&self) -> Result<GameTicket> {
        Ok(GameTicket {
            doc_ticket: self.state.ticket().await?,
            room_id: self.name.clone(),
        })
    }

    /// Start the Game
    pub async fn start_game(&self) -> Result<()> {
        if !self.is_host().await? {
            return Err(anyhow::anyhow!("Only the host can start the game"));
        }
        if self.get_app_state().await? != AppState::Lobby {
            return Err(anyhow::anyhow!("Game has already started"));
        }

        let players: PeerMap = self.get_peer_list().await?;
        let roles: HashMap<EndpointId, G::PlayerRole> = self.logic.assign_roles(&players)?;
        if let Some(peer) = players.iter().find_map(|(peer_id, peer)| {
            roles
                .get(peer_id)
                .filter(|role| !self.logic.is_observer_role(role))
                .filter(|_| !peer.ready)
                .map(|_| peer)
        }) {
            return Err(anyhow::anyhow!("Peer {peer} is not ready"));
        }
        self.logic.validate_start(&players, &roles)?;
        let initial_state: G::GameState = self.logic.initial_state(&players, &roles)?;

        for (peer_id, role) in roles.iter() {
            self.state
                .set_peer_observer(peer_id, self.logic.is_observer_role(role))
                .await?;
        }

        // Broadast the initial game state before setting the game to active.
        self.state.set_game_state(&initial_state).await?;
        self.state.set_app_state(&AppState::InGame).await?;
        Ok(())
    }

    /// Create a new GameRoom
    pub async fn create(
        logic: G,
        store_path: Option<PathBuf>,
        name: Option<&str>,
    ) -> Result<(Self, mpsc::Receiver<UiEvent<G>>)> {
        let state = StateData::new(store_path, None).await?;

        // Host immediately sets the initial lobby state and its own ID.
        state
            .set_room_metadata(&state::RoomMetadata::for_game::<G>())
            .await?;
        state.set_app_state(&AppState::Lobby).await?;
        state.set_host(&state.endpoint_id).await?;

        let mut room = Self::new(state, logic, name.unwrap_or_else(|| G::GAME_NAME));
        let (event_inbox, event_handle) = room.start_event_loop().await?;
        room.event_handle = Some(event_handle);
        Ok((room, event_inbox))
    }

    /// Join a GameRoom
    pub async fn join(
        logic: G,
        ticket: &str,
        store_path: Option<PathBuf>,
    ) -> Result<(Self, mpsc::Receiver<UiEvent<G>>)> {
        let ticket = GameTicket::from_str(ticket)?;
        let room_name = ticket.room_id.clone();
        let state = StateData::new(store_path, Some(ticket)).await?;
        state
            .wait_for_valid_room_metadata(Duration::from_secs(5))
            .await?;

        let mut room = Self::new(state, logic, &room_name);
        let (event_inbox, event_handle) = room.start_event_loop().await?;
        room.event_handle = Some(event_handle);
        Ok((room, event_inbox))
    }

    /// Check whether this room instance is the current host.
    pub async fn is_host(&self) -> Result<bool> {
        self.state.is_host().await
    }

    /// Claim hosting authority for this room if there is no other online host.
    pub async fn claim_host(&self) -> Result<()> {
        self.state.claim_host(&self.logic).await
    }

    /// Get the current application lifecycle state.
    pub async fn get_app_state(&self) -> Result<AppState> {
        self.state.get_app_state().await
    }

    /// Get the latest host-authored game state.
    pub async fn get_game_state(&self) -> Result<G::GameState> {
        self.state.get_game_state().await
    }

    /// Get the latest known peer list.
    pub async fn get_peer_list(&self) -> Result<PeerMap> {
        self.state.get_peer_list().await
    }

    /// Announce this peer's profile to the room.
    pub async fn announce_presence<I: Into<PeerProfile>>(&self, introduction: I) -> Result<()> {
        self.state.announce_presence(introduction).await
    }

    /// Announce this peer's profile and enter the lobby as not ready.
    ///
    /// This is the default lobby path for interactive clients. New peers start
    /// as not ready, so callers can set readiness later in response to user
    /// intent or game-specific automation.
    pub async fn enter_lobby<I: Into<PeerProfile>>(&self, introduction: I) -> Result<()> {
        self.announce_presence(introduction).await
    }

    /// Update this peer's lobby readiness.
    pub async fn set_ready(&self, ready: bool) -> Result<()> {
        self.state.set_peer_ready(&self.id(), ready).await
    }

    /// Send a chat message to room participants.
    pub async fn send_chat(&self, message: &str) -> Result<()> {
        self.state.send_chat(message).await
    }

    /// Get persisted chat messages for this room, ordered oldest to newest.
    pub async fn get_chat_history(&self) -> Result<Vec<ChatMessage>> {
        self.state.get_chat_history().await
    }

    /// Submit a game action for the host to validate and apply.
    ///
    /// This performs local lifecycle checks before publishing the request so UI
    /// callers get immediate feedback for obviously invalid states. The host
    /// still performs authoritative validation when the request is processed.
    pub async fn submit_action(&self, action: G::GameAction) -> Result<()> {
        match self.get_app_state().await? {
            AppState::InGame => {}
            AppState::Lobby => return Err(anyhow::anyhow!("Cannot submit action from lobby")),
            AppState::Paused => return Err(anyhow::anyhow!("Cannot submit action while paused")),
            AppState::Finished => {
                return Err(anyhow::anyhow!(
                    "Cannot submit action after game has finished"
                ));
            }
        }

        match self.state.get_peer_info(&self.id()).await? {
            Some(peer) if peer.is_observer => {
                return Err(anyhow::anyhow!("Peer is an observer"));
            }
            Some(_) => {}
            None => return Err(anyhow::anyhow!("Peer has not joined the room")),
        }

        self.state.submit_action(action).await
    }

    /// Announce that this peer has forfeited active play.
    pub async fn forfeit(&self) -> Result<()> {
        self.state.announce_forfeit().await
    }

    /// Announce that this peer is leaving the room, then drop it.
    pub async fn announce_leave(self, reason: &LeaveReason<G>) -> Result<()> {
        self.state.announce_leave(reason).await
    }
}
