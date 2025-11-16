//! Game Room

mod chat;
mod events;
mod state;

use crate::GameLogic;
use anyhow::Result;
use iroh::EndpointId;
use iroh_docs::{DocTicket, Entry};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::{ops::Deref, path::PathBuf};

pub use events::GameEvent;
pub use state::{AppState, PlayerInfo, PlayerMap, StateData};

#[derive(Clone)]
pub struct GameRoom<G: GameLogic> {
    /// Persistent data store
    pub(self) state: StateData,
    /// Game logic
    pub(self) logic: Arc<G>,
    pub(self) event_handle: Option<Arc<tokio::task::JoinHandle<()>>>,
}

impl<G: GameLogic> Drop for GameRoom<G> {
    fn drop(&mut self) {
        if let Some(handle) = self.event_handle.take() {
            handle.abort();
        }
    }
}

impl<G: GameLogic> Deref for GameRoom<G> {
    type Target = StateData;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl<G: GameLogic> GameRoom<G> {
    /// Get Iroh Network Endpoint ID
    pub fn id(&self) -> EndpointId {
        self.my_id
    }
    /// Read this room's join ticket
    pub fn ticket(&self) -> &DocTicket {
        &self.ticket
    }
    /// Convert entry to known data type
    pub async fn parse<'a, T: DeserializeOwned>(&self, entry: &'a Entry) -> Result<T> {
        self.iroh.get_content_as(entry).await
    }
    /// Create a new game room
    pub async fn create(logic: G, save_path: Option<PathBuf>) -> Result<Self> {
        let logic = Arc::new(logic);
        let state = StateData::new(save_path, None).await?;

        // Host immediately sets the initial lobby state and its own ID.
        state.set_app_state(&AppState::Lobby).await?;
        state.claim_host().await?;
        Ok(GameRoom {
            state,
            logic,
            event_handle: None,
        })
    }
    /// Join an existing game room
    pub async fn join(logic: G, save_path: Option<PathBuf>, ticket: String) -> Result<Self> {
        let logic = Arc::new(logic);
        let state = StateData::new(save_path, Some(ticket)).await?;
        Ok(GameRoom {
            state,
            logic,
            event_handle: None,
        })
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
        let players = self.get_players().await?.unwrap_or_default();

        let current_state: G::GameState = self.get_game_state::<G>().await?;

        self.logic.start_conditions_met(&players, &current_state)?;

        let roles = self.logic.assign_roles(&players);
        let initial_state: G::GameState = self.logic.initial_state(&roles);

        // Broadast the initial game state before setting the game to active.
        self.set_game_state::<G>(&initial_state).await?;
        self.set_app_state(&AppState::InGame).await?;
        Ok(())
    }
}
