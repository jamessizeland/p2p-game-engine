use super::*;
use crate::{GameLogic, PlayerInfo, PlayerMap};
use anyhow::Result;
use n0_future::StreamExt;

impl<G: GameLogic> StateData<G> {
    /// Check the document to see if we are the host
    pub async fn is_host(&self) -> Result<bool> {
        self.is_peer_host(&self.endpoint_id).await
    }

    /// Check the document to see if a given peer is the host
    pub async fn is_peer_host(&self, peer_id: &EndpointId) -> Result<bool> {
        if let Some(bytes) = self.get_bytes(KEY_HOST_ID).await? {
            let host_id_str = String::from_utf8_lossy(&bytes);
            Ok(peer_id.to_string() == host_id_str)
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
    pub async fn get_players_list(&self) -> Result<PlayerMap> {
        let query = self.doc.get_many(Query::all().key_prefix(PREFIX_PLAYER));
        let mut entries = Box::pin(query.await?);
        let mut players = PlayerMap::default();
        while let Some(entry_result) = entries.next().await {
            let entry = entry_result?;
            let player_info: PlayerInfo = self.iroh.get_content_as(&entry).await?;
            let key_str = String::from_utf8_lossy(entry.key());
            let id_str = key_str
                .strip_prefix(std::str::from_utf8(PREFIX_PLAYER)?)
                .unwrap();
            let player_id = EndpointId::from_str(id_str)?;
            players.insert(player_id, player_info);
        }
        Ok(players)
    }

    /// Get a player's Information from their endpointId, if they exist.
    pub async fn get_player_info(&self, player_id: &EndpointId) -> Result<Option<PlayerInfo>> {
        let key = format!("{}{}", std::str::from_utf8(PREFIX_PLAYER)?, player_id);
        if let Some(bytes) = self.get_bytes(key.as_bytes()).await? {
            return Ok(Some(postcard::from_bytes(&bytes)?));
        }
        Ok(None)
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
