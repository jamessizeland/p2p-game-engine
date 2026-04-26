//! Metadata describing the room's protocol and game type, used to detect incompatible clients.

use serde::{Deserialize, Serialize};

use crate::GameLogic;

/// Current protocol version. This should be incremented whenever a breaking change is made to the protocol.
const PROTOCOL_VERSION: u32 = 1;

/// Metadata describing the room's protocol and game type.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RoomMetadata {
    /// Protocol version, used to detect incompatible clients.
    pub protocol_version: u32,
    /// The Rust type name of the game logic, used to detect incompatible clients.
    pub game_type: String,
}

impl RoomMetadata {
    /// Build metadata for the current game logic type.
    pub fn for_game<G: GameLogic>() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            game_type: std::any::type_name::<G>().to_string(),
        }
    }
}
