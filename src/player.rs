use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

use iroh::EndpointId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
    Online,
    Offline,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerInfo {
    pub name: String,
    pub status: PlayerStatus,
}

impl Display for PlayerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Default for PlayerInfo {
    fn default() -> Self {
        Self {
            name: "Unknown".to_string(),
            status: PlayerStatus::Online,
        }
    }
}

impl Into<PlayerInfo> for &str {
    fn into(self) -> PlayerInfo {
        PlayerInfo {
            name: self.to_string(),
            status: PlayerStatus::Online,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PlayerMap(HashMap<EndpointId, PlayerInfo>);

impl Deref for PlayerMap {
    type Target = HashMap<EndpointId, PlayerInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PlayerMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for PlayerMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (id, player) in self.0.iter() {
            let mut id = id.to_string();
            id.truncate(10);
            write!(f, "[{}...]: '{}'\n", id, player)?;
        }
        Ok(())
    }
}
