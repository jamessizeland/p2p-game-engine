#![doc = include_str!("../README.md")]

mod iroh;
mod logic;
mod peer;
mod room;

use iroh::Iroh;
pub use iroh_docs::DocTicket;
pub use logic::{ConnectionEffect, GameLogic};
pub use peer::{PeerInfo, PeerMap, PeerProfile, PeerStatus};
pub use room::{
    ActionResult, AppState, ChatMessage, GameRoom, GameTicket, HostEvent, LeaveReason,
    RoomSnapshot, UiError, UiEvent,
};
