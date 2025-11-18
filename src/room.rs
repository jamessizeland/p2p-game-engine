//! Game Room

mod chat;
mod events;
mod state;

use crate::GameLogic;
use anyhow::Result;
use iroh::EndpointId;
use iroh_docs::DocTicket;
use std::collections::HashMap;
use std::sync::Arc;
use std::{ops::Deref, path::PathBuf};
use tokio::sync::mpsc;

pub use events::GameEvent;
pub use state::{AppState, PlayerInfo, PlayerMap, StateData};

pub struct GameRoom<G: GameLogic> {
    /// Persistent data store
    pub(self) state: Arc<StateData<G>>,
    /// Game logic
    pub(self) logic: Arc<G>,
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
    /// Get Iroh Network Endpoint ID
    pub fn id(&self) -> EndpointId {
        self.endpoint_id
    }
    /// Read this room's join ticket
    pub fn ticket(&self) -> &DocTicket {
        &self.ticket
    }
    /// Create a new game room
    pub async fn create(
        logic: G,
        save_path: PathBuf,
    ) -> Result<(Self, mpsc::Receiver<GameEvent<G>>)> {
        let state = StateData::new(save_path, None).await?;

        // Host immediately sets the initial lobby state and its own ID.
        state.set_app_state(&AppState::Lobby).await?;
        state.claim_host().await?;
        let mut room = GameRoom {
            state: Arc::new(state),
            logic: Arc::new(logic),
            event_handle: None,
        };
        let (event_inbox, event_handle) = room.start_event_loop().await?;
        room.event_handle = Some(event_handle);
        Ok((room, event_inbox))
    }
    /// Join an existing game room
    pub async fn join(
        logic: G,
        ticket: String,
        save_path: PathBuf,
    ) -> Result<(Self, mpsc::Receiver<GameEvent<G>>)> {
        // TODO establish that this ticket matches the game we expect.
        let state = StateData::new(save_path, Some(ticket)).await?;
        let mut room = GameRoom {
            state: Arc::new(state),
            logic: Arc::new(logic),
            event_handle: None,
        };
        let (event_inbox, event_handle) = room.start_event_loop().await?;
        room.event_handle = Some(event_handle);
        Ok((room, event_inbox))
    }
    /// Start the Game
    pub async fn start_game(&self) -> Result<()> {
        if !self.is_host().await? {
            return Err(anyhow::anyhow!("Only the host can start the game"));
        }
        // TODO add mechanism for getting and checking that all players are ready
        if self.get_app_state().await? != AppState::Lobby {
            return Err(anyhow::anyhow!("Game has already started"));
        }

        let players: PlayerMap = self.get_players_list().await?.unwrap_or_default();
        let roles: HashMap<EndpointId, G::PlayerRole> = self.logic.assign_roles(&players);
        let initial_state: G::GameState = self.logic.initial_state(&roles);
        self.logic.start_conditions_met(&players, &initial_state)?;

        // Broadast the initial game state before setting the game to active.
        self.set_game_state(&initial_state).await?;
        self.set_app_state(&AppState::InGame).await?;
        Ok(())
    }
}
