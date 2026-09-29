//! Startup encoding, shared by all joining peers. No per-peer artifact copy or
//! filesystem read; the reactor retains only a frame Arc and a transfer cursor.
use crate::content::Catalog;
use crate::protocol::{self, BundleIdentity, MAX_BUNDLE_PART, ServerMessage};
use crate::server::script::package::client::ClientBundle;
use std::{io, sync::Arc};

pub(super) struct BundleHandshake {
    pub(super) identity: BundleIdentity,
    pub(super) offer: Arc<[u8]>,
    pub(super) parts: Arc<[Arc<[u8]>]>,
}

impl BundleHandshake {
    pub(super) fn new(bundle: &ClientBundle, catalog: &Catalog) -> io::Result<Self> {
        let identity = BundleIdentity {
            client_runtime: crate::protocol::CLIENT_RUNTIME_VERSION,
            key: bundle.cache_key(),
            total_len: bundle.bytes().len() as u32,
        };
        let encode = |message| {
            let mut bytes = Vec::new();
            protocol::write_server_with_catalog(&mut bytes, &message, catalog)?;
            Ok::<Arc<[u8]>, io::Error>(Arc::from(bytes))
        };
        let offer = encode(ServerMessage::BundleOffer { identity })?;
        let parts = bundle
            .bytes()
            .chunks(MAX_BUNDLE_PART)
            .enumerate()
            .map(|(index, bytes)| {
                encode(ServerMessage::BundlePart {
                    offset: (index * MAX_BUNDLE_PART) as u32,
                    bytes: bytes.to_vec(),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            identity,
            offer,
            parts: Arc::from(parts),
        })
    }
}
