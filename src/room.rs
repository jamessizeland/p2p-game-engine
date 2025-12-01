//! Game Room

mod chat;
mod events;
mod state;

use crate::{GameLogic, PeerMap};
use anyhow::Result;
use iroh::EndpointId;
use iroh_docs::DocTicket;
use std::collections::HashMap;
use std::sync::Arc;
use std::{ops::Deref, path::PathBuf};
use tokio::sync::mpsc;

pub use chat::ChatMessage;
pub use events::{HostEvent, UiEvent};
pub use state::{AppState, LeaveReason, StateData};

pub struct GameRoom<G: GameLogic> {
    /// Persistent data store
    pub(self) state: Arc<StateData<G>>,
    /// Game logic
    pub(self) logic: Arc<G>,
    /// UI event loop handle
    pub(self) event_handle: Option<tokio::task::JoinHandle<()>>,
}

impl<G: GameLogic> Drop for GameRoom<G> {
    fn drop(&mut self) {
        if let Some(handle) = self.event_handle.take() {
            handle.abort();
        }
    }
}

impl<G: GameLogic> Deref for GameRoom<G> {
    type Target = StateData<G>;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<G: GameLogic> GameRoom<G> {
    fn new(state: StateData<G>, logic: G) -> Self {
        Self {
            state: Arc::new(state),
            logic: Arc::new(logic),
            event_handle: None,
        }
    }

    /// Get Iroh Network Endpoint ID
    pub fn id(&self) -> EndpointId {
        self.endpoint_id
    }
    /// Get a fresh join ticket for this room, including all known peer addresses.
    pub async fn ticket(&self) -> Result<DocTicket> {
        self.state.ticket().await
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
        let roles: HashMap<EndpointId, G::PlayerRole> = self.logic.assign_roles(&players);
        let initial_state: G::GameState = self.logic.initial_state(&roles);
        self.logic.start_conditions_met(&players, &initial_state)?;

        // Broadast the initial game state before setting the game to active.
        self.set_game_state(&initial_state).await?;
        self.set_app_state(&AppState::InGame).await?;
        Ok(())
    }

    /// Create a new GameRoom
    pub async fn create(
        logic: G,
        store_path: Option<PathBuf>,
    ) -> Result<(Self, mpsc::Receiver<UiEvent<G>>)> {
        let state = StateData::new(store_path, None).await?;

        // Host immediately sets the initial lobby state and its own ID.
        state.set_app_state(&AppState::Lobby).await?;
        state.claim_host().await?;

        let mut room = Self::new(state, logic);
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
        // TODO establish that this ticket matches the game we expect.
        let state = StateData::new(store_path, Some(ticket.to_string())).await?;

        let mut room = Self::new(state, logic);
        let (event_inbox, event_handle) = room.start_event_loop().await?;
        room.event_handle = Some(event_handle);
        Ok((room, event_inbox))
    }
}
