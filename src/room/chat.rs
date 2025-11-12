use crate::state::*;
use crate::{GameLogic, GameRoom};
use anyhow::Result;

impl<G: GameLogic + Send + Sync + 'static> GameRoom<G> {
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
}
