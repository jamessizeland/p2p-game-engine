//! A ticket for joining a game room, including the Iroh document ticket and game/room identifiers.

use std::{fmt, str::FromStr};

use iroh_docs::DocTicket;
use serde::{Deserialize, Serialize};

/// A ticket for joining a game room
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameTicket {
    /// The Iroh network ticket for joining the room, including all known peer addresses.
    pub doc_ticket: DocTicket,
    /// The room ID, used to identify the specific room to join.
    pub room_id: String,
}

impl FromStr for GameTicket {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let game_ticket = serde_json::from_str(s)?;
        Ok(game_ticket)
    }
}

impl TryInto<String> for GameTicket {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<String, Self::Error> {
        let s = serde_json::to_string(&self)?;
        Ok(s)
    }
}

impl fmt::Display for GameTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| fmt::Error)?;
        f.write_str(&s)
    }
}
