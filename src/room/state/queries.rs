use super::*;
use crate::GameLogic;
use anyhow::Result;

impl<G: GameLogic> StateData<G> {
    /// Check the document to see if we are the host
    pub async fn is_host(&self) -> Result<bool> {
        if let Some(bytes) = self.get_bytes(KEY_HOST_ID).await? {
            let host_id = String::from_utf8_lossy(&bytes);
            Ok(self.my_id.to_string() == host_id)
        } else {
            Ok(false)
        }
    }

    /// Get the AppState.
    pub async fn get_app_state(&self) -> Result<AppState> {
        if let Some(bytes) = self.get_bytes(KEY_APP_STATE).await? {
            Ok(postcard::from_bytes(&bytes)?)
        } else {
            Err(anyhow::anyhow!("No AppState found"))
        }
    }

    /// Get Game State.
    pub async fn get_game_state(&self) -> Result<G::GameState> {
        if let Some(bytes) = self.get_bytes(KEY_GAME_STATE).await? {
            Ok(postcard::from_bytes(&bytes)?)
        } else {
            Err(anyhow::anyhow!("No GameState found"))
        }
    }

    /// Get list of players in this Game Room.
    pub async fn get_players(&self) -> Result<Option<PlayerMap>> {
        if let Some(bytes) = self.get_bytes(KEY_PLAYERS).await? {
            Ok(postcard::from_bytes(&bytes)?)
        } else {
            Ok(None)
        }
    }
}

impl<G: GameLogic> StateData<G> {
    /// Query the state data for a particular key
    async fn get_bytes(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let query = self
            .doc
            .get_one(Query::single_latest_per_key().key_exact(key));
        Ok(match query.await? {
            None => None,
            Some(entry) => Some(self.iroh.get_content_bytes(&entry).await?),
        })
    }
}
