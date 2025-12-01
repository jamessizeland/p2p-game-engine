use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

use iroh::EndpointId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    Online,
    Offline,
}

/// Personalisation Information about this peer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PeerProfile {
    /// Name used to introduce the peer
    pub nickname: String,
    /// Avatar URL
    pub avatar: Option<String>,
}

impl Into<PeerProfile> for &str {
    fn into(self) -> PeerProfile {
        PeerProfile {
            nickname: self.to_string(),
            avatar: None,
        }
    }
}

/// General Information about this peer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub id: EndpointId,
    pub profile: PeerProfile,
    pub status: PeerStatus,
    pub ready: bool,
    pub is_observer: bool,
}

impl Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.profile.nickname)
    }
}

impl PeerInfo {
    pub fn new(id: EndpointId, profile: PeerProfile) -> Self {
        Self {
            id,
            profile,
            status: PeerStatus::Online,
            ready: false,
            is_observer: true,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerMap(HashMap<EndpointId, PeerInfo>);

impl Deref for PeerMap {
    type Target = HashMap<EndpointId, PeerInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PeerMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for PeerMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (id, peer_info) in self.0.iter() {
            let mut id = id.to_string();
            id.truncate(10);
            write!(f, "[{}...]: '{}'\n", id, peer_info)?;
        }
        Ok(())
    }
}
