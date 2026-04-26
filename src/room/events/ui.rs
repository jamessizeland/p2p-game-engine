use std::fmt::Display;

use crate::{ActionResult, AppState, ChatMessage, GameLogic, HostEvent, PeerMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent<G: GameLogic> {
    Peer(PeerMap),
    GameState(G::GameState),
    AppState(AppState),
    Chat { sender: String, msg: ChatMessage },
    ActionResult(ActionResult),
    Host(HostEvent),
    Error(String), // TODO replace with AppError including G::GameError
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
            UiEvent::Error(msg) => write!(f, "Error({msg})"),
        }
    }
}
