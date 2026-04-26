use super::*;
use crate::room::state::actions::processed_action_key;
use crate::{GameLogic, PeerInfo, PeerMap, PeerStatus};
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

    /// Get the ID of the endpoint registered as host.
    pub async fn get_host_id(&self) -> Result<EndpointId> {
        if let Some(bytes) = self.get_bytes(KEY_HOST_ID).await? {
            let host_id_str = String::from_utf8_lossy(&bytes);
            Ok(EndpointId::from_str(&host_id_str)?)
        } else {
            Err(anyhow::anyhow!("No HostId found"))
        }
    }

    /// Get the AppState.
    pub async fn get_app_state(&self) -> Result<AppState> {
        if self.is_host_disconnected() {
            return Ok(AppState::Paused);
        };
        if let Some(bytes) = self.get_host_authored_bytes(KEY_APP_STATE).await? {
            Ok(postcard::from_bytes(&bytes)?)
        } else {
            Err(anyhow::anyhow!("No AppState found"))
        }
    }

    /// Get Game State.
    pub async fn get_game_state(&self) -> Result<G::GameState> {
        if let Some(bytes) = self.get_host_authored_bytes(KEY_GAME_STATE).await? {
            Ok(postcard::from_bytes(&bytes)?)
        } else {
            Err(anyhow::anyhow!("No GameState found"))
        }
    }

    /// Get list of peers in this Game Room.
    pub async fn get_peer_list(&self) -> Result<PeerMap> {
        let query = self
            .doc
            .get_many(Query::single_latest_per_key().key_prefix(PREFIX_PEER));
        let mut entries = Box::pin(query.await?);
        let mut peers = PeerMap::default();
        while let Some(entry_result) = entries.next().await {
            let entry = entry_result?;
            let peer_info: PeerInfo = match self.iroh()?.get_content_as(&entry).await {
                Ok(info) => info,
                Err(_) => continue, // TODO is this okay to skip over?
            };
            let key_str = String::from_utf8_lossy(entry.key());
            let id_str = key_str
                .strip_prefix(std::str::from_utf8(PREFIX_PEER)?)
                .expect("Key format should be valid from previous query");
            let Ok(peer_id) = EndpointId::from_str(id_str) else {
                continue;
            };
            peers.insert(peer_id, peer_info);
        }
        if self.is_host_disconnected() {
            // modify the host's status to indicate that they are offline
            if let Ok(host_id) = self.get_host_id().await
                && let Some(host) = peers.get_mut(&host_id)
            {
                host.status = PeerStatus::Offline;
            }
        }
        Ok(peers)
    }

    /// Get a peer's Information from their endpointId, if they exist.
    pub async fn get_peer_info(&self, peer_id: &EndpointId) -> Result<Option<PeerInfo>> {
        let key = format!("{}{}", std::str::from_utf8(PREFIX_PEER)?, peer_id);
        if let Some(bytes) = self.get_bytes(key.as_bytes()).await? {
            return Ok(Some(postcard::from_bytes(&bytes)?));
        }
        Ok(None)
    }
    /// Get a peer's name from their endpointId, if they exist.
    pub async fn get_peer_name(&self, peer_id: &EndpointId) -> Result<String> {
        let peer_info = self.get_peer_info(peer_id).await?;
        Ok(peer_info.map_or("unknown".to_string(), |peer| peer.profile.nickname))
    }

    /// Check whether an action request has already been processed.
    pub(crate) async fn has_processed_action(
        &self,
        peer_id: &EndpointId,
        action_id: &str,
    ) -> Result<bool> {
        Ok(self
            .get_bytes(&processed_action_key(peer_id, action_id)?)
            .await?
            .is_some())
    }

    /// Check whether an entry author matches the registered peer author.
    pub(crate) async fn peer_author_matches(
        &self,
        peer_id: &EndpointId,
        author_id: &AuthorId,
    ) -> Result<bool> {
        Ok(self
            .get_peer_info(peer_id)
            .await?
            .is_some_and(|peer| &peer.author_id == author_id))
    }

    /// Check whether an entry author matches the current host author.
    pub(crate) async fn host_author_matches(&self, author_id: &AuthorId) -> Result<bool> {
        let Ok(host_id) = self.get_host_id().await else {
            return Ok(true);
        };
        match self.get_peer_info(&host_id).await {
            Ok(Some(peer)) => Ok(&peer.author_id == author_id),
            Ok(None) | Err(_) => Ok(true),
        }
    }

    /// Choose the next online peer that can take over hosting.
    pub(crate) async fn next_host_candidate(
        &self,
        excluding: &EndpointId,
    ) -> Result<Option<EndpointId>> {
        let peers = self.get_peer_list().await?;
        let mut candidates: Vec<_> = peers
            .iter()
            .filter(|(id, peer)| {
                *id != excluding && peer.status == PeerStatus::Online && !peer.is_observer
            })
            .map(|(id, _)| *id)
            .collect();
        // if no non-observer peers are available, allow observers to be candidates as well
        if candidates.is_empty() {
            candidates = peers
                .iter()
                .filter(|(id, peer)| *id != excluding && peer.status == PeerStatus::Online)
                .map(|(id, _)| *id)
                .collect();
        }
        // sort candidates by their string representation to ensure deterministic selection of the next host
        candidates.sort_by_key(|id| id.to_string());
        Ok(candidates.into_iter().next())
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
            Some(entry) => Some(self.iroh()?.get_content_bytes(&entry).await?),
        })
    }

    /// Get the latest bytes for a key written by the current host.
    async fn get_host_authored_bytes(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let Some(host_author) = self.get_host_author_id().await? else {
            return self.get_bytes(key).await;
        };
        let query = self.doc.get_many(Query::key_exact(key));
        let mut entries = Box::pin(query.await?);
        let mut latest = None;
        while let Some(entry_result) = entries.next().await {
            let entry = entry_result?;
            if entry.author() != host_author {
                continue;
            }
            if latest
                .as_ref()
                .is_none_or(|current: &Entry| entry.timestamp() > current.timestamp())
            {
                latest = Some(entry);
            }
        }
        Ok(match latest {
            Some(entry) => Some(self.iroh()?.get_content_bytes(&entry).await?),
            None => self.get_bytes(key).await?,
        })
    }

    /// Get the registered document author for the current host, if known.
    async fn get_host_author_id(&self) -> Result<Option<AuthorId>> {
        let Ok(host_id) = self.get_host_id().await else {
            return Ok(None);
        };
        Ok(self
            .get_peer_info(&host_id)
            .await
            .ok()
            .flatten()
            .map(|peer| peer.author_id))
    }
}
