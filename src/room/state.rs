//! State information

mod actions;
mod queries;

use anyhow::Result;
use bytes::Bytes;
use iroh::EndpointId;
use iroh_docs::{DocTicket, api::protocol::ShareMode, store::Query};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, str::FromStr as _};

use crate::Iroh;

// --- Key Prefixes ---
pub(self) const KEY_APP_STATE: &[u8] = b"app_state";
pub(self) const KEY_HOST_ID: &[u8] = b"host_id";
pub(self) const KEY_PLAYERS: &[u8] = b"players";
pub(self) const KEY_GAME_STATE: &[u8] = b"game_state";
pub(self) const KEY_HEARTBEAT: &[u8] = b"heartbeat";
pub(self) const PREFIX_JOIN: &[u8] = b"join_request.";
pub(self) const PREFIX_ACTION: &[u8] = b"action.";
pub(self) const PREFIX_CHAT: &[u8] = b"chat.";
pub(self) const PREFIX_PLAYER_READY: &[u8] = b"player_ready.";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInfo {
    pub name: String,
}

pub type PlayerMap = HashMap<EndpointId, PlayerInfo>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum AppState {
    Lobby,
    InGame,
    Finished,
}

/// Wrapper for the Iroh Document
#[derive(Clone)]
pub struct StateData {
    doc: iroh_docs::api::Doc,
    pub(crate) author_id: iroh_docs::AuthorId,
    pub(crate) my_id: EndpointId,
    iroh: Iroh,
    pub(crate) ticket: DocTicket,
}

impl StateData {
    /// Create a new StateData instance
    pub async fn new(store_path: Option<PathBuf>, ticket: Option<String>) -> Result<Self> {
        let dir = store_path.unwrap_or(tempfile::tempdir()?.path().to_path_buf());
        let iroh = Iroh::new(dir).await?;
        let my_id = iroh.endpoint().id();
        if let Some(ticket) = ticket {
            let ticket = DocTicket::from_str(&ticket)?;
            let doc = iroh.docs().import(ticket.clone()).await?;
            let author_id = iroh.setup_author(&doc.id()).await?;
            Ok(Self {
                doc,
                author_id,
                my_id,
                iroh,
                ticket,
            })
        } else {
            let doc = iroh.docs().create().await?;
            let author_id = iroh.setup_author(&doc.id()).await?;
            let ticket = doc.share(ShareMode::Write, Default::default()).await?;
            Ok(Self {
                doc,
                author_id,
                my_id,
                iroh,
                ticket,
            })
        }
    }
}
