use crate::{
    GameLogic, GameRoom, PlayerInfo,
    room::{AppState, PlayerMap, chat::ChatMessage, state::*},
};
use anyhow::{Result, anyhow};
use iroh_blobs::Hash;
use iroh_docs::{ContentStatus, engine::LiveEvent, sync::Entry};
use n0_future::StreamExt as _;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Public events your library will send to the game UI
#[derive(Debug)]
pub enum GameEvent<G: GameLogic> {
    LobbyUpdated(PlayerMap),
    StateUpdated(G::GameState),
    AppStateChanged(AppState),
    ChatReceived { id: PlayerInfo, msg: ChatMessage },
    HostDisconnected,
    Error(String),
}

impl<G: GameLogic> Display for GameEvent<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameEvent::LobbyUpdated(players) => write!(f, "LobbyUpdated({players})"),
            GameEvent::StateUpdated(state) => write!(f, "StateUpdated({state:?})"),
            GameEvent::AppStateChanged(state) => write!(f, "AppStateChanged({state:?})"),
            GameEvent::ChatReceived { id: _, msg } => write!(f, "ChatReceived({msg:?})"),
            GameEvent::HostDisconnected => write!(f, "HostDisconnected"),
            GameEvent::Error(msg) => write!(f, "Error({msg})"),
        }
    }
}

impl<G: GameLogic> GameRoom<G> {
    pub(crate) async fn start_event_loop(
        &mut self,
    ) -> Result<(mpsc::Receiver<GameEvent<G>>, JoinHandle<()>)> {
        let mut sub = self.state.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let state_data = self.state.clone();
        let logic = self.logic.clone();

        let task_handle = tokio::spawn(async move {
            let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();
            loop {
                tokio::select! {
                    // Listen for iroh doc events
                    Some(Ok(event)) = sub.next() => {
                        if let Some(entry) = parse_live_event(event, &mut pending_entries) {
                            match process_entry(&entry, &state_data, &logic).await {
                                Ok(None) => {} // No event to send
                                Err(e) => eprintln!("Error processing event: {}", e),
                                Ok(Some(event)) => {
                                    if sender.send(event).await.is_err() {
                                        break; // Channel closed
                                    }
                                }
                            }
                        }
                    },
                    else => break, // Stream finished
                }
            }
        });
        Ok((receiver, task_handle))
    }
}

async fn process_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<GameEvent<G>>> {
    let is_host = data.is_host().await?;

    #[cfg(debug_assertions)]
    {
        let mut key = String::from_utf8_lossy(entry.key()).to_string();
        key.truncate(15);
        println!(
            ">> {} >> Processing entry: {key}",
            if is_host { "HOST" } else { "CLIENT" },
        );
    }

    // --- HOST LOGIC ---
    if let Some(node_id) = entry.is_join() {
        if !is_host {
            return Ok(None);
        }
        let node_id = node_id?;
        // A player has joined the game room.
        // Get the PlayerInfo payload
        let player_info = match data.parse::<PlayerInfo>(&entry).await {
            Ok(info) => info,
            Err(e) => {
                return Err(anyhow!("Failed to parse PlayerInfo for {}: {e}", &node_id,));
            }
        };
        // Broadcast the new canonical player list
        data.insert_player(node_id, &player_info).await?;
        // The `insert_player` will trigger a `player_entry` live event, which will
        // in turn trigger the `LobbyUpdated` ui event. So we don't need to return anything here.
        return Ok(None);
    } else if let Some(node_id) = entry.is_action_request() {
        if !is_host {
            return Ok(None);
        }
        let node_id = node_id?;
        // Ensure we have a state to apply the action to
        let current_state = &mut data.get_game_state().await?;

        match data.parse::<G::GameAction>(&entry).await {
            Ok(action) => {
                // Apply the game logic and broadcast the new authoritative state
                match logic.apply_action(current_state, &node_id, &action) {
                    Err(e) => {
                        let player = data.get_player_info(&node_id).await?.unwrap_or_default();
                        return Err(anyhow!("Invalid action from {player}: {e}"));
                    }
                    Ok(()) => data.set_game_state(current_state).await?,
                };
            }
            Err(e) => {
                let player = data.get_player_info(&node_id).await?.unwrap_or_default();
                return Err(anyhow!("Failed to parse GameAction from {player}: {e}",));
            }
        }
        // The `set_game_state`` will trigger a `game_state_update` live event, which will
        // in turn trigger the `StateUpdated` ui event. So we don't need to return anything here.
        return Ok(None);
    }
    // --- ALL-PLAYERS LOGIC ---
    if let Some(node_id) = entry.is_chat_message() {
        let node_id = node_id?;
        let player = data.get_player_info(&node_id).await?.unwrap_or_default();
        return match data.parse::<ChatMessage>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse ChatMessage from {player}: {e}")),
            Ok(msg) => Ok(Some(GameEvent::ChatReceived {
                id: player.clone(),
                msg,
            })),
        };
    } else if entry.is_player_entry() {
        // A player entry has been added/updated. Fetch the whole list to signal an update.
        return match data.get_players_list().await {
            Err(e) => Err(anyhow!("Failed to get players list after update: {e}")),
            Ok(players) => Ok(Some(GameEvent::LobbyUpdated(players))),
        };
    } else if entry.is_game_state_update() {
        // The game state has been updated by the host.
        return match data.parse::<G::GameState>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse GameState: {e}")),
            Ok(state) => Ok(Some(GameEvent::StateUpdated(state))),
        };
    } else if entry.is_app_state_update() {
        // The app state has been updated by the host.
        return match data.parse::<AppState>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse AppState: {e}")),
            Ok(app_state) => Ok(Some(GameEvent::AppStateChanged(app_state))),
        };
    }
    println!("unexpected event {}", String::from_utf8_lossy(entry.key()));

    Ok(None)
}

/// Output a doc entry when a new one is ready.
fn parse_live_event(event: LiveEvent, pending_entries: &mut HashMap<Hash, Entry>) -> Option<Entry> {
    use ContentStatus::{Complete, Incomplete, Missing};
    match event {
        // TODO maybe add functionality to handle losing and gaining neighbours
        LiveEvent::InsertLocal { entry } => Some(entry),
        LiveEvent::InsertRemote {
            entry,
            content_status: Complete,
            ..
        } => Some(entry),
        LiveEvent::InsertRemote {
            entry,
            content_status: Missing | Incomplete,
            ..
        } => {
            pending_entries.insert(entry.content_hash(), entry);
            None
        }
        LiveEvent::ContentReady { hash } => pending_entries.remove(&hash),
        _other => {
            #[cfg(debug_assertions)]
            println!("LIVE_EVENT >>> {_other:?}");
            None
        }
    }
}
