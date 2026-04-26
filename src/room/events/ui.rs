use std::fmt::Display;

use crate::{ActionResult, AppState, ChatMessage, GameLogic, HostEvent, PeerMap};

/// UI events that the game room emits to the application layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    SyncFailed(String),
    EventProcessing {
        key: String,
        author: String,
        message: String,
    },
}

impl Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::SyncFailed(reason) => write!(f, "Sync failed: {reason}"),
            UiError::EventProcessing { key, message, .. } => {
                write!(f, "Failed to process event '{key}': {message}")
            }
        }
    }
}

/// UI events that the game room emits to the application layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent<G: GameLogic> {
    Peer(PeerMap),
    GameState(G::GameState),
    AppState(AppState),
    Chat { sender: String, msg: ChatMessage },
    ActionResult(ActionResult),
    Host(HostEvent),
    Error(UiError),
}

impl<G: GameLogic> Display for UiEvent<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiEvent::Peer(peers) => write!(f, "PeerUpdated({peers})"),
            UiEvent::GameState(state) => write!(f, "GameStateUpdated({state:?})"),
            UiEvent::AppState(state) => write!(f, "AppStateChanged({state:?})"),
            UiEvent::Chat { sender: _, msg } => write!(f, "Chat({msg:?})"),
            UiEvent::ActionResult(result) => write!(f, "ActionResult({result:?})"),
            UiEvent::Host(HostEvent::Changed { to }) => write!(f, "HostSet({to})"),
            UiEvent::Host(HostEvent::Offline) => write!(f, "HostOffline"),
            UiEvent::Host(HostEvent::Online) => write!(f, "HostOnline"),
            UiEvent::Error(error) => write!(f, "Error({error:?})"),
        }
    }
}
