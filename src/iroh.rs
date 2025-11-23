use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6},
    path::PathBuf,
};

use anyhow::Result;
use bytes::Bytes;
use iroh::SecretKey;
use iroh::protocol::Router;
use iroh_blobs::{
    ALPN as BLOBS_ALPN, BlobsProtocol,
    api::{Store, blobs::Blobs},
    store::{fs::FsStore, mem::MemStore},
};
use iroh_docs::{ALPN as DOCS_ALPN, AuthorId, protocol::Docs};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use serde::de::DeserializeOwned;

#[derive(Clone, Debug)]
pub struct Iroh {
    router: Router,
    blobs: Blobs,
    path: Option<PathBuf>,
    docs: Docs,
}

impl Iroh {
    /// Shared internal builder
    async fn build(
        endpoint: iroh::Endpoint,
        store: Store,
        docs: Docs,
        gossip: Gossip,
        path: Option<PathBuf>,
    ) -> Result<Self> {
        // Get the generic client interface
        let blobs = store.blobs().clone();
        let router = iroh::protocol::Router::builder(endpoint)
            .accept(BLOBS_ALPN, BlobsProtocol::new(&store, None))
            .accept(GOSSIP_ALPN, gossip)
            .accept(DOCS_ALPN, docs.clone())
            .spawn();
        Ok(Self {
            router,
            docs,
            path,
            blobs,
        })
    }

    /// Create an In-Memory Iroh Node (Strictly for Tests)
    pub async fn memory() -> Result<Self> {
        let key = load_secret_key(None).await?; // Generate random key

        // Bind to Random Port (0) to prevent test collisions
        let endpoint = iroh::Endpoint::builder()
            .secret_key(key)
            .bind_addr_v4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))
            .bind_addr_v6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0))
            .bind()
            .await?;
        let gossip = Gossip::builder().spawn(endpoint.clone());
        // Setup Memory Store
        let blobs_store: Store = MemStore::new().into();

        // Setup Memory Docs
        let docs = Docs::memory()
            .spawn(endpoint.clone(), blobs_store.clone(), gossip.clone())
            .await?;

        Self::build(endpoint, blobs_store, docs, gossip, None).await
    }

    /// Create a Persistent Iroh Node (For the actual App)
    pub async fn persistent(path: PathBuf) -> Result<Self> {
        // create dir if it doesn't already exist
        tokio::fs::create_dir_all(&path).await?;
        let key = load_secret_key(Some(path.clone().join("keypair"))).await?;

        // Bind to default port 11204, or fail if taken (standard app behavior)
        let endpoint = iroh::Endpoint::builder().secret_key(key).bind().await?;
        let gossip = Gossip::builder().spawn(endpoint.clone());
        // Setup Persistent Store
        let blobs_store: Store = FsStore::load(&path).await?.into();
        // Setup Persistent Docs
        let docs = Docs::persistent(path.clone())
            .spawn(endpoint.clone(), blobs_store.clone(), gossip.clone())
            .await?;

        Self::build(endpoint, blobs_store, docs, gossip, Some(path)).await
    }

    /// Retrieve or create a persistent Default Author for this node
    pub async fn get_default_author(&self) -> Result<AuthorId> {
        // Use a fixed filename so we reuse the identity across different games
        let Some(root) = &self.path else {
            return Ok(self.docs().author_create().await?);
        };
        let author_path = root.join("default.author");
        if author_path.exists() {
            let bytes = tokio::fs::read(&author_path).await?;
            let author = iroh_docs::Author::from_bytes(
                &bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid author file"))?,
            );
            // Import the author into the internal docs store to make it active
            self.docs().author_import(author.clone()).await?;
            Ok(author.id())
        } else {
            let new_author = self.docs().author_create().await?;
            let Some(persisting_author) = self.docs().author_export(new_author).await? else {
                return Err(anyhow::anyhow!("failed to export author"));
            };
            tokio::fs::write(author_path, persisting_author.to_bytes()).await?;
            Ok(new_author)
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub async fn get_content_bytes(&self, entry: &iroh_docs::sync::Entry) -> Result<Bytes> {
        Ok(self.blobs().get_bytes(entry.content_hash()).await?)
    }
    pub async fn get_content_as<'a, T: DeserializeOwned>(
        &self,
        entry: &'a iroh_docs::sync::Entry,
    ) -> Result<T> {
        let bytes = self.get_content_bytes(entry).await?;
        Ok(postcard::from_bytes(&bytes)?)
    }

    pub fn endpoint(&self) -> &iroh::Endpoint {
        self.router.endpoint()
    }

    pub fn blobs(&self) -> &Blobs {
        &self.blobs
    }

    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    pub async fn shutdown(self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }
}

/// Helper to load key from disk OR generate if path is None
async fn load_secret_key(key_path: Option<PathBuf>) -> Result<SecretKey> {
    let Some(key_path) = key_path else {
        return Ok(SecretKey::generate(&mut rand::rng()));
    };
    if key_path.exists() {
        let key_bytes = tokio::fs::read(key_path).await?;
        return Ok(SecretKey::try_from(&key_bytes[0..32])?);
    }

    let secret_key = SecretKey::generate(&mut rand::rng());
    // Try to canonicalize if possible
    let key_path = key_path.canonicalize().unwrap_or(key_path);
    let key_path_parent = key_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent directory found for '{}'", key_path.display()))?;
    tokio::fs::create_dir_all(&key_path_parent).await?;
    tokio::fs::write(&key_path, &secret_key.to_bytes()).await?;
    Ok(secret_key)
}
