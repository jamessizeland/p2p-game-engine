//! State information
//!
//! This module contains the `StateData` struct, which is the main interface for interacting with the game state.
//! It also contains the `GameKey` trait, which is used to parse entries in the document and determine what type
//! of event they represent.
//!
//! The `StateData` struct provides methods for setting and getting various pieces of state information, such as the
//! current app state, game state, room metadata, and peer information. It also provides methods for checking if
//! certain events have occurred, such as if a peer has joined or quit, if an action has been requested or processed,
//! and if a chat message has been sent.

mod actions;
mod game_key;
mod lifecycle;
mod metadata;
mod queries;

use crate::{GameLogic, Iroh};
use anyhow::{Result, anyhow};
use bytes::Bytes;
use iroh::EndpointId;
use iroh_docs::api::{
    Doc,
    protocol::{AddrInfoOptions, ShareMode},
};
use iroh_docs::store::Query;
use iroh_docs::{AuthorId, DocTicket, Entry};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    marker::PhantomData,
    path::PathBuf,
    str::FromStr as _,
    sync::{Arc, atomic::AtomicBool},
};

pub use actions::{ActionRequest, ActionResult};
pub use game_key::GameKey;
pub use lifecycle::{AppState, LeaveReason};
pub use metadata::RoomMetadata;

/// Wrapper for the Iroh Document
#[derive(Clone)]
pub struct StateData<G: GameLogic> {
    /// If we are not the host, and the host is offline, we pause.
    host_disconnected: Arc<AtomicBool>,
    phantom: PhantomData<G>,
    pub(crate) endpoint_id: EndpointId,
    pub(crate) author_id: AuthorId,
    // ticket: DocTicket,
    iroh: Option<Iroh>,
    pub(crate) doc: Doc,
}

/// Convert a string to an EndpointId, returning an error if the string is not a valid EndpointId.
pub fn endpoint_id_from_str(id: &str) -> Result<EndpointId> {
    EndpointId::from_str(id).map_err(|err| anyhow!("Invalid EndpointId from key {}: {}", id, err))
}

// --- Key Prefixes ---
/// Key for the current AppState, set by the host.
const KEY_APP_STATE: &[u8] = b"app_state";
/// Key for the current GameState, set by the host.
const KEY_HOST_ID: &[u8] = b"host_id";
/// Key for the current GameState, set by the host.
const KEY_GAME_STATE: &[u8] = b"game_state";
/// Key for the room metadata, set by the host.
const KEY_ROOM_METADATA: &[u8] = b"room_metadata";
/// Prefix for a peer entry, which contains information about a peer in the room.
const PREFIX_JOIN: &[u8] = b"join_request.";
/// Prefix for a peer quit announcement.
const PREFIX_QUIT: &[u8] = b"quit_request.";
/// Prefix for an action request entry.
const PREFIX_ACTION: &[u8] = b"action.";
/// Prefix for an action result entry, which contains the result of an action request.
const PREFIX_ACTION_RESULT: &[u8] = b"action_result.";
/// Prefix for a processed action entry, which contains the result of an action request after it has been processed by the host.
const PREFIX_PROCESSED_ACTION: &[u8] = b"processed_action.";
/// Prefix for a chat message entry.
const PREFIX_CHAT: &[u8] = b"chat.";
/// Prefix for a peer entry, which contains information about a peer in the room.
const PREFIX_PEER: &[u8] = b"peer.";
