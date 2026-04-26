//! Read-only room snapshots for UI redraws.

use crate::{AppState, GameLogic, GameRoom, PeerMap};
use anyhow::Result;
use iroh::EndpointId;

/// A point-in-time view of the room state that is convenient for UI rendering.
///
/// `RoomSnapshot` gathers the commonly-needed room queries into one value so
/// applications do not need to fan out across several async calls every time
/// they redraw. `game_state` is `None` while the game has not published an
/// initial host-authored state yet, which is normal during the lobby.
#[derive(Debug, Clone)]
pub struct RoomSnapshot<G: GameLogic> {
    /// The endpoint ID of this local room instance.
    pub local_id: EndpointId,
    /// The endpoint ID currently registered as host, if one is known.
    pub host_id: Option<EndpointId>,
    /// Whether this local room instance is the current host.
    pub is_host: bool,
    /// Whether the current host is known to be offline by this room instance.
    pub host_disconnected: bool,
    /// The current application lifecycle state, including synthetic pause.
    pub app_state: AppState,
    /// The latest known peer map, including synthetic host offline status.
    pub peers: PeerMap,
    /// The latest host-authored game state, if one has been published.
    pub game_state: Option<G::GameState>,
}

impl<G: GameLogic> GameRoom<G> {
    /// Get a point-in-time view of room data used by UI redraws.
    pub async fn snapshot(&self) -> Result<RoomSnapshot<G>> {
        Ok(RoomSnapshot {
            local_id: self.id(),
            host_id: self.state.get_host_id().await.ok(),
            is_host: self.is_host().await?,
            host_disconnected: self.state.is_host_disconnected(),
            app_state: self.get_app_state().await?,
            peers: self.get_peer_list().await?,
            game_state: self.get_game_state().await.ok(),
        })
    }
}
