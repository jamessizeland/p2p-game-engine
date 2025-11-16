//! Game Room

mod actions;
mod chat;
mod events;
mod queries;
mod setup;

use crate::{GameLogic, iroh::Iroh};
use iroh::EndpointId;
use iroh_docs::{AuthorId, api::Doc};
use std::sync::Arc;

pub use events::GameEvent;
pub use setup::{RoomBuilder, RoomEntry};

#[derive(Clone)]
pub struct GameRoom<G: GameLogic> {
    /// Networking interface
    pub(self) iroh: Iroh,
    /// Persistent data store
    pub(self) doc: Doc,
    /// Document writer unique identifier
    pub(self) author: AuthorId,
    /// Game logic
    pub(self) logic: Arc<G>,
    /// Whether we are the host
    is_host: bool,
    /// Our location unique identifier
    id: EndpointId,
}

impl<G: GameLogic> GameRoom<G> {
    /// Get Iroh Network Endpoint ID
    pub fn id(&self) -> EndpointId {
        self.id
    }
    /// Is this gameroom instance hosting?
    pub fn is_host(&self) -> bool {
        // TODO we should be checking this from the document not from our local state, incase the host has migrated.
        self.is_host
    }
}
