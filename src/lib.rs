#![doc = include_str!("../README.md")]

mod logic;
mod networking;
mod peer;
mod room;

pub use logic::{ConnectionEffect, GameLogic};
use networking::Iroh;
pub use peer::{PeerInfo, PeerMap, PeerProfile, PeerStatus};
pub use room::{
    ActionResult, AppState, ChatMessage, GameRoom, GameTicket, HostEvent, LeaveReason,
    RoomSnapshot, UiError, UiEvent,
};

pub mod iroh {
    //! Re-exports of the Iroh library, including the main `Iroh` struct for interacting with the network,
    //! as well as the `DocTicket` struct for working with documents in the Docs protocol.
    pub use iroh::*;
    pub use iroh_docs::DocTicket;
}
