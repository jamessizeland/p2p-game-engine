use std::time::Duration;

use super::*;
use crate::{ChatMessage, GameLogic, PeerInfo, PeerProfile, PeerStatus};
use anyhow::Result;
use tokio::time::sleep;

impl<G: GameLogic> StateData<G> {
    /// Set the AppState.
    pub async fn set_app_state(&self, state: &AppState) -> Result<()> {
        let state = postcard::to_stdvec(&state)?;
        self.set_bytes(KEY_APP_STATE, &state).await
    }

    /// Set Game State.
    pub async fn set_game_state(&self, state: &G::GameState) -> Result<()> {
        let state = postcard::to_stdvec(state)?;
        self.set_bytes(KEY_GAME_STATE, &state).await
    }

    /// Declare that this endpoint now has hosting authority.
    pub async fn claim_host(&self) -> Result<()> {
        // TODO improve logic here, we need to check if another online peer already has hosting authority.
        self.set_bytes(KEY_HOST_ID, self.endpoint_id.to_string().as_bytes())
            .await
    }

    /// Send a chat message.
    pub async fn send_chat(&self, message: &str) -> Result<()> {
        let message = ChatMessage::new(self.endpoint_id, message)?;
        // Key ensures uniqueness for last-write-wins conflict resolution
        // e.g., "chat.123456789.id"
        let chat_key = format!(
            "{}{}.{}",
            std::str::from_utf8(PREFIX_CHAT)?,
            message.timestamp,
            self.endpoint_id
        );
        let value = postcard::to_stdvec(&message)?;
        self.set_bytes(&chat_key.into_bytes(), &value).await
    }

    /// Add a peer to the peers list
    pub async fn insert_peer(&self, peer_id: &EndpointId, profile: PeerProfile) -> Result<()> {
        let peer_info = PeerInfo::new(*peer_id, profile);
        self.update_peer(peer_id, peer_info).await
    }

    /// Update a peer's info, or add them if they don't exist.
    pub async fn update_peer(&self, peer_id: &EndpointId, peer_info: PeerInfo) -> Result<()> {
        let key = format!("{}{}", std::str::from_utf8(PREFIX_PEER)?, peer_id);
        let value = postcard::to_stdvec(&peer_info)?;
        self.set_bytes(key.as_bytes(), &value).await
    }

    /// Set a peer's online/offline status, if they are in our peer list
    pub async fn set_peer_status(&self, peer_id: &EndpointId, status: PeerStatus) -> Result<()> {
        if let Some(mut peer_info) = self.get_peer_info(peer_id).await? {
            peer_info.status = status;
            self.update_peer(peer_id, peer_info).await?;
        }
        Ok(())
    }

    /// Announce that we have left the room, and why.
    pub async fn announce_leave(self, reason: &LeaveReason<G>) -> Result<()> {
        let quit_key = format!("{}{}", str::from_utf8(PREFIX_QUIT)?, self.endpoint_id);
        let value = postcard::to_stdvec(reason)?;
        self.set_bytes(&quit_key.into_bytes(), &value).await?;
        // allow a short delay for this message to sync
        sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    /// Announce that we have joined the room.
    pub async fn announce_presence(&self, introduction: impl Into<PeerProfile>) -> Result<()> {
        let join_key = format!("{}{}", str::from_utf8(PREFIX_JOIN)?, self.endpoint_id);
        let value = postcard::to_stdvec(&introduction.into())?;
        self.set_bytes(&join_key.into_bytes(), &value).await
    }

    /// Submit a game action.
    pub async fn submit_action(&self, action: G::GameAction) -> Result<()> {
        // Key is "action.id" - this will overwrite previous actions,
        // which is fine as the host processes them sequentially.
        let action_key = format!("{}{}", str::from_utf8(PREFIX_ACTION)?, self.endpoint_id);
        let value = postcard::to_stdvec(&action)?;
        self.set_bytes(&action_key.into_bytes(), &value).await
    }
}

impl<G: GameLogic> StateData<G> {
    /// Set the state data for a particular key.
    async fn set_bytes(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.doc
            .set_bytes(self.author_id, key.to_vec(), value.to_vec())
            .await?;
        Ok(())
    }
}
