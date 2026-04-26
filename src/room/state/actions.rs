use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::{ChatMessage, GameLogic, PeerInfo, PeerMap, PeerProfile, PeerStatus};
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
        self.set_host(&self.endpoint_id).await
    }

    /// Declare that a peer now has hosting authority.
    pub(crate) async fn set_host(&self, peer_id: &EndpointId) -> Result<()> {
        self.set_bytes(KEY_HOST_ID, peer_id.to_string().as_bytes())
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
    pub(crate) async fn insert_peer(
        &self,
        peer_id: &EndpointId,
        author_id: AuthorId,
        profile: PeerProfile,
    ) -> Result<()> {
        let peer_info = PeerInfo::new(*peer_id, author_id, profile);
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

    /// Set a peer's observer flag if they are in the peer list.
    pub(crate) async fn set_peer_observer(
        &self,
        peer_id: &EndpointId,
        is_observer: bool,
    ) -> Result<()> {
        if let Some(mut peer_info) = self.get_peer_info(peer_id).await? {
            peer_info.is_observer = is_observer;
            self.update_peer(peer_id, peer_info).await?;
        }
        Ok(())
    }

    /// Announce that we have left the room, and why.
    pub async fn announce_leave(&self, reason: &LeaveReason<G>) -> Result<()> {
        let quit_key = format!("{}{}", str::from_utf8(PREFIX_QUIT)?, self.endpoint_id);
        let value = postcard::to_stdvec(reason)?;
        self.set_bytes(&quit_key.into_bytes(), &value).await?;
        // allow a short delay for this message to sync
        sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    /// Announce that this peer has forfeited active play.
    pub async fn announce_forfeit(&self) -> Result<()> {
        let reason = LeaveReason::<G>::Forfeit;
        self.announce_leave(&reason).await
    }

    /// Announce that we have joined the room.
    pub async fn announce_presence(&self, introduction: impl Into<PeerProfile>) -> Result<()> {
        let join_key = format!("{}{}", str::from_utf8(PREFIX_JOIN)?, self.endpoint_id);
        let value = postcard::to_stdvec(&introduction.into())?;
        self.set_bytes(&join_key.into_bytes(), &value).await
    }

    /// Submit a game action.
    pub async fn submit_action(&self, action: G::GameAction) -> Result<()> {
        let action_id = unique_id()?;
        let action_key = format!(
            "{}{}.{}",
            str::from_utf8(PREFIX_ACTION)?,
            self.endpoint_id,
            action_id
        );
        let value = postcard::to_stdvec(&ActionRequest {
            id: action_id,
            action,
        })?;
        self.set_bytes(&action_key.into_bytes(), &value).await
    }

    /// Publish the host's accept/reject result for an action request.
    pub(crate) async fn set_action_result(
        &self,
        peer_id: &EndpointId,
        result: &ActionResult,
    ) -> Result<()> {
        let key = format!(
            "{}{}.{}",
            str::from_utf8(PREFIX_ACTION_RESULT)?,
            peer_id,
            result.action_id
        );
        let value = postcard::to_stdvec(result)?;
        self.set_bytes(key.as_bytes(), &value).await
    }

    /// Mark an action request as already handled by the host.
    pub(crate) async fn mark_action_processed(
        &self,
        peer_id: &EndpointId,
        action_id: &str,
    ) -> Result<()> {
        let key = processed_action_key(peer_id, action_id)?;
        self.set_bytes(&key, &[1]).await
    }

    /// Persist all peer entries from a modified peer map.
    pub(crate) async fn persist_peer_list(&self, players: &PeerMap) -> Result<()> {
        for (peer_id, peer_info) in players.iter() {
            self.update_peer(peer_id, peer_info.clone()).await?;
        }
        Ok(())
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

/// Build the document key used to record a processed action.
pub(crate) fn processed_action_key(peer_id: &EndpointId, action_id: &str) -> Result<Vec<u8>> {
    Ok(format!(
        "{}{}.{}",
        str::from_utf8(PREFIX_PROCESSED_ACTION)?,
        peer_id,
        action_id
    )
    .into_bytes())
}

/// Generate a locally unique action identifier.
fn unique_id() -> Result<String> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(format!("{nanos}"))
}
