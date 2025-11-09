use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Key Prefixes (as constants) ---
pub const KEY_APP_STATE: &[u8] = b"app_state";
pub const KEY_HOST_ID: &[u8] = b"host_id";
pub const KEY_PLAYERS: &[u8] = b"players";
pub const KEY_GAME_STATE: &[u8] = b"game_state";
pub const PREFIX_JOIN: &[u8] = b"join_request.";
pub const PREFIX_ACTION: &[u8] = b"action.";
pub const PREFIX_CHAT: &[u8] = b"chat.";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInfo {
    pub name: String,
}

// Using a type alias for the player list
pub type PlayerMap = HashMap<EndpointId, PlayerInfo>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub from: EndpointId,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AppState {
    Lobby,
    InGame,
    Finished,
}
