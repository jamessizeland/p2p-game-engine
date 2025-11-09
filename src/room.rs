mod actions;
mod events;
mod setup;

use crate::{GameLogic, iroh::Iroh};
use iroh::EndpointId;
use iroh_docs::{AuthorId, api::Doc};
use std::sync::Arc;

pub use events::GameEvent;

pub struct GameRoom<G: GameLogic> {
    pub(self) iroh: Iroh,
    pub(self) doc: Doc,
    pub(self) author: AuthorId,
    pub(self) logic: Arc<G>,
    pub(self) is_host: bool,
    pub(self) id: EndpointId,
}
