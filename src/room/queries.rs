use crate::{
    GameLogic, GameRoom,
    state::{AppState, KEY_APP_STATE, KEY_GAME_STATE, KEY_PLAYERS, PlayerMap},
};
use anyhow::Result;
use iroh_docs::store::Query;

impl<G: GameLogic> GameRoom<G> {
    /// Gets the current game state from the document, if it exists.
    pub async fn get_game_state(&self) -> Result<Option<G::GameState>> {
        if let Some(entry) = self
            .doc
            .get_one(Query::single_latest_per_key().key_exact(KEY_GAME_STATE))
            .await?
        {
            let state = self.iroh.get_content_as(&entry).await?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    /// Gets the current player list from the document, if it exists.
    pub async fn get_players(&self) -> Result<Option<PlayerMap>> {
        if let Some(entry) = self
            .doc
            .get_one(Query::single_latest_per_key().key_exact(KEY_PLAYERS))
            .await?
        {
            let players = self.iroh.get_content_as(&entry).await?;
            Ok(Some(players))
        } else {
            Ok(None)
        }
    }

    /// Gets the current application state from the document, if it exists.
    pub async fn get_app_state(&self) -> Result<Option<AppState>> {
        if let Some(entry) = self
            .doc
            .get_one(Query::single_latest_per_key().key_exact(KEY_APP_STATE))
            .await?
        {
            let state = self.iroh.get_content_as(&entry).await?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }
}
