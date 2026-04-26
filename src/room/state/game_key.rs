//! GameKey trait and implementation for Entry
//!
//! The `GameKey` trait is used to parse entries in the document and determine
//! what type of event they represent, such as a join request, action request,
//! chat message, etc. The implementation for `Entry` provides methods to check
//! if an entry matches a particular event type and extract relevant information
//! from the key.

use super::*;
use anyhow::{Result, anyhow};
use iroh::EndpointId;
use iroh_docs::Entry;

pub trait GameKey {
    /// This entry is an arrival announcement, return the ID of the new arrival.
    fn is_join(&self) -> Option<Result<EndpointId>>;
    /// This entry is a request to perform an action, return the requestor and action id.
    fn is_action_request(&self) -> Option<Result<(EndpointId, String)>>;
    /// This entry is the result of a requested action, return the requestor and action id.
    fn is_action_result(&self) -> Option<Result<(EndpointId, String)>>;
    /// This entry is a chat message, return the ID of the sender.
    fn is_chat_message(&self) -> Option<Result<EndpointId>>;
    /// This entry is a quit announcement, return the ID of the quitter.
    fn is_quit_request(&self) -> Option<Result<EndpointId>>;
    /// A peer entry has been updated
    fn is_peer_entry(&self) -> bool;
    /// Game State has updated
    fn is_game_state_update(&self) -> bool;
    /// App State has updated
    fn is_app_state_update(&self) -> bool;
    /// Host has updated
    fn is_host_update(&self) -> bool;
}

impl GameKey for Entry {
    fn is_join(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_JOIN) {
            return None;
        }
        let id = String::from_utf8_lossy(&self.key()[PREFIX_JOIN.len()..]);
        Some(endpoint_id_from_str(&id))
    }
    fn is_action_request(&self) -> Option<Result<(EndpointId, String)>> {
        if !self.key().starts_with(PREFIX_ACTION) {
            return None;
        }
        Some(parse_endpoint_and_suffix(&String::from_utf8_lossy(
            &self.key()[PREFIX_ACTION.len()..],
        )))
    }
    fn is_action_result(&self) -> Option<Result<(EndpointId, String)>> {
        if !self.key().starts_with(PREFIX_ACTION_RESULT) {
            return None;
        }
        Some(parse_endpoint_and_suffix(&String::from_utf8_lossy(
            &self.key()[PREFIX_ACTION_RESULT.len()..],
        )))
    }
    fn is_chat_message(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_CHAT) {
            return None;
        }
        // The key is "chat.<timestamp>.<id>", so we split and take the last part.
        let key_str = String::from_utf8_lossy(self.key());
        key_str.split('.').next_back().map(endpoint_id_from_str)
    }
    fn is_quit_request(&self) -> Option<Result<EndpointId>> {
        if !self.key().starts_with(PREFIX_QUIT) {
            return None;
        }
        let id = String::from_utf8_lossy(&self.key()[PREFIX_QUIT.len()..]);
        Some(endpoint_id_from_str(&id))
    }
    fn is_peer_entry(&self) -> bool {
        self.key().starts_with(PREFIX_PEER)
    }
    fn is_game_state_update(&self) -> bool {
        self.key() == KEY_GAME_STATE
    }
    fn is_app_state_update(&self) -> bool {
        self.key() == KEY_APP_STATE
    }
    fn is_host_update(&self) -> bool {
        self.key() == KEY_HOST_ID
    }
}

/// Parse keys shaped as `<endpoint>.<suffix>`.
fn parse_endpoint_and_suffix(value: &str) -> Result<(EndpointId, String)> {
    let Some((id, suffix)) = value.split_once('.') else {
        return Err(anyhow!("Expected '<endpoint>.<id>', got '{value}'"));
    };
    Ok((endpoint_id_from_str(id)?, suffix.to_string()))
}
