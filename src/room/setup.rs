use crate::{AppState, GameRoom, KEY_APP_STATE, KEY_HOST_ID};
use crate::{GameLogic, iroh::Iroh};
use anyhow::Result;
use iroh_docs::{DocTicket, api::protocol::ShareMode};

use std::str::FromStr as _;
use std::sync::Arc;

impl<G: GameLogic + Send + Sync + 'static> GameRoom<G> {
    /// HOST: Creates a new game lobby.
    pub async fn host(iroh: Iroh, logic: G) -> Result<(Self, DocTicket)> {
        let author = iroh.docs().author_create().await?;
        let doc = iroh.docs().create().await?;

        // Generate the ticket
        let ticket = doc.share(ShareMode::Write, Default::default()).await?;
        let my_id = iroh.endpoint().id();

        let app_state_bytes =
            postcard::to_stdvec(&AppState::Lobby).expect("Failed to serialize initial AppState");
        // Host immediately sets the initial lobby state
        doc.set_bytes(author, KEY_APP_STATE, app_state_bytes)
            .await?;
        doc.set_bytes(author, KEY_HOST_ID, my_id.to_vec()).await?;

        Ok((
            Self {
                iroh,
                doc,
                author,
                logic: Arc::new(logic),
                is_host: true,
                id: my_id,
            },
            ticket,
        ))
    }

    /// JOIN: Joins an existing game lobby.
    pub async fn join(iroh: Iroh, logic: G, ticket: String) -> Result<Self> {
        let author = iroh.docs().author_create().await?;
        let ticket = DocTicket::from_str(&ticket)?;
        let doc = iroh.docs().import(ticket).await?;
        let my_id = iroh.endpoint().id();

        // Application is responsible for announcing presence.

        Ok(Self {
            iroh,
            doc,
            author,
            logic: Arc::new(logic),
            is_host: false,
            id: my_id,
        })
    }
}
