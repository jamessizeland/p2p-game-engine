use std::collections::HashMap;

use iroh::EndpointId;
use iroh_blobs::Hash;
use iroh_docs::{
    Entry,
    engine::{LiveEvent, SyncEvent},
};

#[derive(Debug)]
pub enum NetworkEvent {
    Update(Entry),
    Joiner(EndpointId),
    Leaver(EndpointId),
    SyncFailed(String),
    SyncSucceeded,
}

impl NetworkEvent {
    /// Output a doc entry when a new one is ready.
    pub fn parse(event: LiveEvent, pending_entries: &mut HashMap<Hash, Entry>) -> Option<Self> {
        use iroh_docs::ContentStatus::{Complete, Incomplete, Missing};
        match event {
            LiveEvent::InsertLocal { entry } => Some(Self::Update(entry)),
            LiveEvent::InsertRemote {
                entry,
                content_status: Complete,
                ..
            } => Some(Self::Update(entry)),
            LiveEvent::InsertRemote {
                entry,
                content_status: Missing | Incomplete,
                ..
            } => {
                pending_entries.insert(entry.content_hash(), entry);
                None
            }
            LiveEvent::ContentReady { hash } => pending_entries.remove(&hash).map(Self::Update),
            LiveEvent::NeighborUp(id) => Some(Self::Joiner(id)),
            LiveEvent::NeighborDown(id) => Some(Self::Leaver(id)),
            LiveEvent::SyncFinished(SyncEvent { result, .. }) => match result {
                Ok(_) => Some(Self::SyncSucceeded),
                Err(reason) => Some(Self::SyncFailed(reason)),
            },
            _other => None,
        }
    }
}
