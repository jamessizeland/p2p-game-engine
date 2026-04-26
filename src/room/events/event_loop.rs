use super::{
    network::NetworkEvent,
    ui::{UiError, UiEvent},
};
use crate::{
    GameLogic, GameRoom,
    room::{
        events::process::{process_joiner, process_leaver, process_update},
        state::StateData,
    },
};
use anyhow::Result;

use iroh_blobs::Hash;
use iroh_docs::{Entry, engine::LiveEvent};
use n0_future::{Stream, StreamExt as _};
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Public events your library will send to the game UI

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    /// Host has connected
    Online,
    /// Host has disconnected
    Offline,
    /// A new host has been assigned
    Changed { to: String },
}

impl<G: GameLogic> GameRoom<G> {
    pub(crate) async fn start_event_loop(
        &mut self,
    ) -> Result<(mpsc::Receiver<UiEvent<G>>, JoinHandle<()>)> {
        let sub = self.state.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let state_data = self.state.clone();
        let logic = self.logic.clone();

        let task_handle = tokio::spawn(async move {
            event_loop(sub, sender, state_data, &logic).await;
        });
        Ok((receiver, task_handle))
    }
}

/// Main event loop that listens for iroh doc events and processes them.
async fn event_loop<G: GameLogic>(
    mut sub: impl Stream<Item = Result<LiveEvent>> + Unpin,
    sender: mpsc::Sender<UiEvent<G>>,
    state_data: Arc<StateData<G>>,
    logic: &Arc<G>,
) {
    let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();
    loop {
        tokio::select! {
            // Listen for iroh doc events
            Some(Ok(event)) = sub.next() => {
                let network_event = match NetworkEvent::parse(event, &mut pending_entries)  {
                    Some(event) => event,
                    None => continue,
                };
                let maybe_event = match network_event {
                    NetworkEvent::Update(entry) => process_update(&entry, &state_data, logic).await,
                    NetworkEvent::Joiner(id) => process_joiner(id, &state_data, logic ).await,
                    NetworkEvent::Leaver(id) => process_leaver(id, &state_data, logic).await,
                    NetworkEvent::SyncFailed(reason) => Some(UiEvent::Error(UiError::SyncFailed(reason))),
                    NetworkEvent::SyncSucceeded => None, /* Do nothing for now */
                };
                if let Some(ui_event) = maybe_event && sender.send(ui_event).await.is_err() {
                    break; // Receiver dropped, exit loop
                }
            },
            else => break, // Stream finished
        }
    }
}
