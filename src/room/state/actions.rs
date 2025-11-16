use super::*;
use crate::{GameLogic, room::chat::ChatMessage};
use anyhow::Result;

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
        self.set_bytes(KEY_HOST_ID, self.my_id.to_string().as_bytes())
            .await
    }

    /// Send a chat message.
    pub async fn send_chat(&self, message: &str) -> Result<()> {
        let message = ChatMessage::new(self.my_id, message)?;
        // Key ensures uniqueness for LWW, e.g., "chat.123456789.id"
        let chat_key = format!(
            "{}{}.{}",
            std::str::from_utf8(PREFIX_CHAT)?,
            message.timestamp,
            self.my_id
        );
        let value = postcard::to_stdvec(&message)?;
        self.set_bytes(&chat_key.into_bytes(), &value).await
    }

    /// Update the list of players.
    pub async fn set_player_list(&self, players: &PlayerMap) -> Result<()> {
        let value = postcard::to_stdvec(players)?;
        self.set_bytes(KEY_PLAYERS, &value).await
    }

    /// Announce that we have left the room, and why.
    pub async fn announce_leave(&self, reason: &LeaveReason) -> Result<()> {
        let quit_key = format!("{}{}", std::str::from_utf8(PREFIX_QUIT)?, self.my_id);
        let value = postcard::to_stdvec(reason)?;
        self.set_bytes(&quit_key.into_bytes(), &value).await
    }

    /// Announce that we have joined the room.
    pub async fn announce_presence(&self, player: impl Into<super::PlayerInfo>) -> Result<()> {
        let join_key = format!("{}{}", std::str::from_utf8(PREFIX_JOIN)?, self.my_id);
        let value = postcard::to_stdvec(&player.into())?;
        self.set_bytes(&join_key.into_bytes(), &value).await
    }

    /// Submit a game action.
    pub async fn submit_action(&self, action: G::GameAction) -> Result<()> {
        // Key is "action.id" - this will overwrite previous actions,
        // which is fine as the host processes them sequentially.
        let action_key = format!("{}{}", std::str::from_utf8(PREFIX_ACTION)?, self.my_id);
        let value = postcard::to_stdvec(&action)?;
        self.set_bytes(&action_key.into_bytes(), &value).await
    }
    /// Set the heartbeat to now
    pub async fn set_heartbeat(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        let value = postcard::to_stdvec(&now)?;
        self.set_bytes(KEY_HEARTBEAT, &value).await
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
