//! Chat messages
//!
//! This module contains the `ChatMessage` struct, which represents a chat message sent by a peer in the game room.
//! It includes the sender's endpoint ID, the message content, and a timestamp for when the message was created.

use std::fmt::Display;

use anyhow::Result;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};

/// A chat message sent by a peer in the game room, containing the sender's endpoint ID, the message content,
/// and a timestamp for when the message was created.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// The ID of the peer who sent this message.
    pub from: EndpointId,
    /// The content of the message.
    pub message: String,
    /// The timestamp for when this message was created, represented as milliseconds since the Unix epoch.
    pub timestamp: u64,
}

impl ChatMessage {
    /// Create a new chat message from the given sender and message content, with the current timestamp.
    pub fn new(from: EndpointId, message: &str) -> Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        Ok(Self {
            from,
            message: message.to_string(),
            timestamp,
        })
    }
}

impl Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.timestamp, self.from, self.message)
    }
}
