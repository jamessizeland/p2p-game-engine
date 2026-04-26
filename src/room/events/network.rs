//! Logic for processing iroh network events and translating them into UI events.
//!
//! This module handles events such as new document entries, neighbor changes, and synchronization results.
//! The main function is `NetworkEvent::parse`, which takes a live event from the iroh engine and produces
//! an optional `NetworkEvent` that can be emitted to the UI.

use std::collections::HashMap;

use iroh::EndpointId;
use iroh_blobs::Hash;
use iroh_docs::{
    Entry,
    engine::{LiveEvent, SyncEvent},
};

/// Network events that can be emitted to the UI.
#[derive(Debug)]
pub enum NetworkEvent {
    /// A new document entry is ready to be processed.
    Update(Entry),
    /// A new peer has joined the room.
    Joiner(EndpointId),
    /// A peer has left the room.
    Leaver(EndpointId),
    /// The synchronization process has failed with a reason.
    SyncFailed(String),
    /// The synchronization process has succeeded.
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
