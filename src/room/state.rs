//! State information

mod actions;
mod queries;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use iroh::EndpointId;
use iroh_docs::{
    AuthorId, DocTicket, Entry,
    api::{Doc, protocol::ShareMode},
    store::Query,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, marker::PhantomData, path::PathBuf, str::FromStr as _};

use crate::{GameLogic, Iroh};

// --- Key Prefixes ---
pub(self) const KEY_APP_STATE: &[u8] = b"app_state";
pub(self) const KEY_HOST_ID: &[u8] = b"host_id";
pub(self) const KEY_PLAYERS: &[u8] = b"players";
pub(self) const KEY_GAME_STATE: &[u8] = b"game_state";
pub(self) const PREFIX_JOIN: &[u8] = b"join_request.";
pub(self) const PREFIX_QUIT: &[u8] = b"quit_request.";
pub(self) const PREFIX_ACTION: &[u8] = b"action.";
pub(self) const PREFIX_CHAT: &[u8] = b"chat.";
#[allow(unused)]
pub(self) const PREFIX_PLAYER_READY: &[u8] = b"player_ready.";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInfo {
    pub name: String,
}

impl Into<PlayerInfo> for &str {
    fn into(self) -> PlayerInfo {
        PlayerInfo {
            name: self.to_string(),
        }
    }
}

pub type PlayerMap = HashMap<EndpointId, PlayerInfo>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
/// Report a reason for this endpoint leaving a GameRoom
pub enum LeaveReason {
    ApplicationClosed,
    Timeout,
    Error(String),
    Custom(String),
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum AppState {
    Lobby,
    InGame,
    Finished,
}

/// Wrapper for the Iroh Document
#[derive(Clone)]
pub struct StateData<G: GameLogic> {
    phantom: PhantomData<G>,
    pub(crate) endpoint_id: EndpointId,
    pub(crate) author_id: AuthorId,
    pub(crate) ticket: DocTicket,
    pub(crate) iroh: Iroh,
    pub(crate) doc: Doc,
}

impl<G: GameLogic> StateData<G> {
    /// Create a new StateData instance
    pub async fn new(store_path: PathBuf, ticket: Option<String>) -> Result<Self> {
        let iroh = Iroh::new(store_path).await?;
        let endpoint_id = iroh.endpoint().id();
        let (ticket, doc, author_id) = if let Some(ticket_str) = ticket {
            let ticket = DocTicket::from_str(&ticket_str)?;
            let doc = iroh.docs().import(ticket.clone()).await?;
            let author_id = iroh.setup_author(&doc.id()).await?;
            (ticket, doc, author_id)
        } else {
            let doc = iroh.docs().create().await?;
            let author_id = iroh.setup_author(&doc.id()).await?;
            let ticket = doc.share(ShareMode::Write, Default::default()).await?;
            (ticket, doc, author_id)
        };
        Ok(Self {
            phantom: PhantomData,
            endpoint_id,
            author_id,
            ticket,
            iroh,
            doc,
        })
    }

    /// Convert entry to known data type
    pub async fn parse<'a, T: DeserializeOwned>(&self, entry: &'a Entry) -> Result<T> {
        self.iroh.get_content_as(entry).await
    }
}

pub trait GameKey {
    /// This entry is an arrival announcement, return the ID of the new arrival.
    fn is_join(&self) -> Option<Result<EndpointId>>;
    /// This entry is a request to perform an action, return the ID of the requestor.
    fn is_action_request(&self) -> Option<Result<EndpointId>>;
    /// This entry is a chat message, return the ID of the sender.
    fn is_chat_message(&self) -> Option<Result<EndpointId>>;
    /// Player map has updated
    fn is_players_update(&self) -> bool;
    /// Game State has updated
    fn is_game_state_update(&self) -> bool;
    /// App State has updated
    fn is_app_state_update(&self) -> bool;
}

impl GameKey for Entry {
    fn is_join(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_JOIN) {
            return None;
        }
        let id = String::from_utf8_lossy(&self.key()[PREFIX_JOIN.len()..]);
        Some(
            EndpointId::from_str(&id)
                .map_err(|err| anyhow!("Invalid EndpointId from key {}: {}", id, err)),
        )
    }
    fn is_action_request(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_ACTION) {
            return None;
        }
        let id = String::from_utf8_lossy(&self.key()[PREFIX_ACTION.len()..]);
        Some(
            EndpointId::from_str(&id)
                .map_err(|err| anyhow!("Invalid EndpointId from key {}: {}", id, err)),
        )
    }
    fn is_chat_message(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_CHAT) {
            return None;
        }
        let id = String::from_utf8_lossy(&self.key()[PREFIX_CHAT.len()..]);
        Some(
            EndpointId::from_str(&id)
                .map_err(|err| anyhow!("Invalid EndpointId from key {}: {}", id, err)),
        )
    }
    fn is_players_update(&self) -> bool {
        self.key() == KEY_PLAYERS
    }
    fn is_game_state_update(&self) -> bool {
        self.key() == KEY_GAME_STATE
    }
    fn is_app_state_update(&self) -> bool {
        self.key() == KEY_APP_STATE
    }
}
