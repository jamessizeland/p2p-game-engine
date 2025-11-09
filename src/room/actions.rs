use crate::state::*;
use crate::{GameLogic, GameRoom};
use anyhow::Result;
use iroh_docs::store::Query;

impl<G: GameLogic + Send + Sync + 'static> GameRoom<G> {
    /// Announce our presence when joining
    pub async fn announce_presence(&self, name: &str) -> Result<()> {
        let join_key = format!("{}{}", std::str::from_utf8(PREFIX_JOIN)?, self.id);

        let payload = PlayerInfo {
            name: name.to_string(),
        };
        let bytes = postcard::to_stdvec(&payload)?;

        self.doc
            .set_bytes(self.author.clone(), join_key.into_bytes(), bytes)
            .await?;
        Ok(())
    }

    /// Send a chat message
    pub async fn send_chat(&self, message: String) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;

        // Key ensures uniqueness for LWW, e.g., "chat.123456789.id"
        let chat_key = format!(
            "{}{}.{}",
            std::str::from_utf8(PREFIX_CHAT)?,
            timestamp,
            self.id
        );

        let payload = ChatMessage {
            from: self.id,
            message,
            timestamp,
        };
        let bytes = postcard::to_stdvec(&payload)?;

        self.doc
            .set_bytes(self.author.clone(), chat_key.into_bytes(), bytes)
            .await?;
        Ok(())
    }

    /// Submit a game action
    pub async fn submit_action(&self, action: G::GameAction) -> Result<()> {
        // Key is "action.id" - this will overwrite previous actions,
        // which is fine as the host processes them sequentially.
        let action_key = format!("{}{}", std::str::from_utf8(PREFIX_ACTION)?, self.id);

        let bytes = postcard::to_stdvec(&action)?;

        self.doc
            .set_bytes(self.author.clone(), action_key.into_bytes(), bytes)
            .await?;
        Ok(())
    }

    /// (HOST-ONLY) Start the game
    pub async fn start_game(&self) -> Result<()> {
        if !self.is_host {
            return Err(anyhow::anyhow!("Only the host can start the game"));
        }

        // 1. Get the current players from our local state
        // We need to read our own "players" key to be sure.
        let players_entry = self
            .doc
            .get_one(Query::single_latest_per_key().key_exact(KEY_PLAYERS))
            .await?
            .ok_or_else(|| anyhow::anyhow!("No players in lobby"))?;

        let players: PlayerMap = self.iroh.get_content_as(&players_entry).await?;

        // 2. Call the user-defined logic to get roles and initial state
        let roles = self.logic.assign_roles(&players);
        let initial_state = self.logic.initial_state(&roles);
        let state_bytes = postcard::to_stdvec(&initial_state)?;

        // 3. Broadcast the initial game state
        self.doc
            .set_bytes(self.author.clone(), KEY_GAME_STATE.to_vec(), state_bytes)
            .await?;

        // 4. Set AppState to InGame
        let app_state_bytes = postcard::to_stdvec(&AppState::InGame)?;
        self.doc
            .set_bytes(self.author.clone(), KEY_APP_STATE.to_vec(), app_state_bytes)
            .await?;

        Ok(())
    }
}
