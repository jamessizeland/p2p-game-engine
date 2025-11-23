use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6},
    path::PathBuf,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use iroh::SecretKey;
use iroh::protocol::Router;
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol, api::blobs::Blobs, store::fs::FsStore};
use iroh_docs::{ALPN as DOCS_ALPN, AuthorId, protocol::Docs};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug)]
pub struct Iroh {
    router: Router,
    store: FsStore,
    path: PathBuf,
    docs: Docs,
}

impl Iroh {
    /// Create a new Iroh Doc service.
    pub async fn new(path: PathBuf, use_random_port: bool) -> Result<Self> {
        // create dir if it doesn't already exist
        tokio::fs::create_dir_all(&path).await?;

        let key = load_secret_key(path.clone().join("keypair")).await?;
        let mut builder = iroh::Endpoint::builder().secret_key(key);
        if use_random_port {
            let v4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
            let v6 = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);
            builder = builder.bind_addr_v4(v4).bind_addr_v6(v6);
        }
        let endpoint = builder.bind().await?;

        let gossip = Gossip::builder().spawn(endpoint.clone());
        let blobs = FsStore::load(&path).await?;
        let docs = Docs::persistent(path.clone())
            .spawn(endpoint.clone(), (*blobs).clone(), gossip.clone())
            .await?;
        // build the protocol router
        let builder = iroh::protocol::Router::builder(endpoint.clone());
        let router = builder
            .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
            .accept(GOSSIP_ALPN, gossip)
            .accept(DOCS_ALPN, docs.clone())
            .spawn();
        Ok(Self {
            router,
            docs,
            path,
            store: blobs,
        })
    }

    /// Retrieve or create a persistent Default Author for this node
    pub async fn get_default_author(&self) -> Result<AuthorId> {
        // Use a fixed filename so we reuse the identity across different games
        let author_path = self.path.join("default.author");

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

    pub fn path(&self) -> &PathBuf {
        &self.path
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
        self.store.blobs()
    }

    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    pub async fn shutdown(self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }
}

async fn load_secret_key(key_path: PathBuf) -> Result<SecretKey> {
    if key_path.exists() {
        let key_bytes = tokio::fs::read(key_path).await?;
        let secret_key = SecretKey::try_from(&key_bytes[0..32])?;

        Ok(secret_key)
    } else {
        let secret_key = SecretKey::generate(&mut rand::rng());

        // Try to canonicalize if possible
        let key_path = key_path.canonicalize().unwrap_or(key_path);
        let key_path_parent = key_path.parent().ok_or_else(|| {
            anyhow::anyhow!("no parent directory found for '{}'", key_path.display())
        })?;
        tokio::fs::create_dir_all(&key_path_parent).await?;

        // write to tempfile
        let (file, temp_file_path) = tempfile::NamedTempFile::new_in(key_path_parent)
            .context("unable to create tempfile")?
            .into_parts();
        let mut file = tokio::fs::File::from_std(file);
        file.write_all(&secret_key.to_bytes())
            .await
            .context("unable to write keyfile")?;
        file.flush().await?;
        drop(file);

        // move file
        tokio::fs::rename(temp_file_path, key_path)
            .await
            .context("failed to rename keyfile")?;

        Ok(secret_key)
    }
}
