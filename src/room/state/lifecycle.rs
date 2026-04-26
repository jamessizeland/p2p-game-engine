//! Defines the data structures and logic for managing the state of a game room,
//! including player actions, game state, and lifecycle events.

use super::*;
use crate::GameLogic;
use anyhow::Result;

/// Report a reason for this endpoint leaving a GameRoom
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum LeaveReason<G: GameLogic> {
    /// Peer has closed the application.
    ApplicationClosed,
    /// Peer has timed out.
    Timeout,
    /// Peer has chosen to end their participation in this game.
    Forfeit,
    /// Something has gone wrong and an error has been reported.
    Error(String),
    /// Something else has happened that is expected.
    Custom(G::PlayerLeaveReason),
    /// An unknown error has occurred.
    Unknown,
}

/// The current state of the game, used to determine what actions are available and how the UI should be presented.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum AppState {
    /// The game is in the lobby, waiting for players to join and the host to start the game.
    /// In this state, players can chat and see who else is in the room, but cannot see the game state or perform actions.
    Lobby,
    /// The game is in progress, and players can perform actions.
    /// In this state, the host is expected to be online and actively processing actions.
    InGame,
    /// The game is paused, either because the host is offline or because the game has been manually paused.
    /// In this state, players cannot perform actions, but can still chat and see the current game state.
    Paused,
    /// The game has ended, either because a win condition has been met or because the host has ended the game.
    /// In this state, players cannot perform actions, but can still chat and see the final game state.
    Finished,
}

impl<G: GameLogic> Drop for StateData<G> {
    fn drop(&mut self) {
        if let Some(iroh) = self.iroh.take() {
            tokio::spawn(async move {
                iroh.shutdown().await.ok();
            });
        }
    }
}

impl<G: GameLogic> StateData<G> {
    /// Ticket option that helps with reconnecting to a ticket instance.
    const ADDR_OPTIONS: AddrInfoOptions = AddrInfoOptions::RelayAndAddresses;

    /// Create a new StateData instance
    pub async fn new(store_path: Option<PathBuf>, ticket: Option<String>) -> Result<Self> {
        let iroh = match store_path {
            None => Iroh::memory().await?,
            Some(store_path) => Iroh::persistent(store_path).await?,
        };
        let author_id = iroh.docs().author_default().await?;
        let endpoint_id = iroh.endpoint().id();

        let (_ticket, doc) = if let Some(ticket_str) = ticket {
            let ticket = DocTicket::from_str(&ticket_str)?;
            let doc = iroh.docs().import(ticket.clone()).await?;
            (ticket, doc)
        } else {
            let doc = iroh.docs().create().await?;
            let ticket = doc.share(ShareMode::Write, Self::ADDR_OPTIONS).await?;
            (ticket, doc)
        };

        Ok(Self {
            host_disconnected: Arc::new(AtomicBool::new(false)),
            phantom: PhantomData,
            endpoint_id,
            author_id,
            iroh: Some(iroh),
            doc,
        })
    }

    pub(crate) fn iroh(&self) -> Result<&Iroh> {
        self.iroh.as_ref().ok_or(anyhow!("Network layer missing"))
    }

    /// Convert entry to known data type
    pub async fn parse<T: DeserializeOwned>(&self, entry: &Entry) -> Result<T> {
        self.iroh()?.get_content_as(entry).await
    }
    /// Set the data into a paused state
    pub fn host_offline(&self) {
        self.host_disconnected
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    /// Set the data into a resumed state
    pub fn host_online(&self) {
        self.host_disconnected
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
    /// Check if the data is in a paused state
    pub fn is_host_disconnected(&self) -> bool {
        self.host_disconnected
            .load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Regenerate the ticket with the latest node information
    pub async fn ticket(&self) -> Result<DocTicket> {
        // Regenerate the ticket to include all current peer addresses.
        let ticket = self.doc.share(ShareMode::Write, Self::ADDR_OPTIONS).await?;
        Ok(ticket)
    }
}
