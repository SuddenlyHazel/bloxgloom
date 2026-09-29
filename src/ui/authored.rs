//! Closed package UI schema. Only verified bytes enter here, on preparation
//! threads before publication. Drawing never evaluates source or performs I/O.
//!
//! Format-2 manifests classify ui-document/ui-style JSON under assets/ui/,
//! ui-font TTF under assets/fonts/, and ui-image PNG under assets/ui/. Documents
//! are {version:1,nodes:[...]}; each node names id, kind, style and an earlier
//! panel's parent index (except the first root). Text/image/event are optional
//! kind-specific fields. All resource/event refs are package:local, restricted
//! to the owning package even when it has declared dependencies. Resolved widget
//! identities are immutable package:document/node strings.
//!
//! Global UI bounds: 8 documents, 256 nodes, 64 styles, 4 fonts, 16 images, 4096
//! text bytes including each input's full 128-byte capacity. Per document: 64
//! nodes, depth 16, 16 KiB JSON. PNGs are static, <=256x256, <=262144 total pixels.
//! Fonts use a restricted TTF profile (see font_limits); the legacy preview
//! rasterizes ASCII glyphs with fontdue. Production egui uses the verified TTF
//! bytes and image atlas. All resources must fit a fixed 1024x1024 RGBA atlas.
//! Existing bundle asset/byte limits also apply.
//!
//! Egui lays out and draws panels, labels, images, buttons and inputs in live
//! play, including bounded scrolling, wrapping and platform text editing. The
//! earlier Taffy layout and atlas painter remain for legacy headless previews.
//! F6 opens/closes; PageDown cycles lexical documents. Reconnect resets local state;
//! close/reopen retains it; document cycling resets it and invalidates old replies.
//!
//! Optional document `presentation:{capability:"local-ui",module:"package:module"}`
//! opts into a package-owned verified client/shared module (no imports). A fresh
//! bounded Luau sandbox runs on the client presentation worker for each button
//! click/Enter or changed input. It returns a function accepting {sequence,event,
//! value,state,texts}; texts maps full widget IDs to current bounded UTF-8 text. Sequence
//! numbers start at 1 per connection, increase on admission, and survive document
//! switches. Script globals never survive; `state` is an explicit 128-byte string.
//! Return a dense array of <=16 commands: {op:"text",node:"package:doc/id",value:
//! "UTF-8"}, {op:"visible",node:...,value:boolean}, or {op:"state",value:"UTF-8"}.
//! Text/state are <=128 bytes. Only the active document is writable. Hidden
//! ancestors hide descendants and remove focus, but retain layout space.
//! An `action` command can request a package-owned registered item/empty/block/entity
//! action; scripts supply its bounded key and optional <=130-byte argument string.
//! The client composes the identity-fenced request from its current selection,
//! streamed target and inventory revision, then the server
//! authorizes/stages the effect and returns a durable action receipt. Host-owned
//! chrome reports pending, denial and acceptance; local script text is not proof
//! of server application. Switching documents invalidates old feedback.
//! Results validate atomically; errors are module/event-attributed and disable
//! handlers until reset. One outstanding event, request and reply queues of one;
//! busy clicks/edits are rejected (visible BUSY status), never queued for retry.
//! Local retained dynamic text is bounded by 64 nodes x 128 bytes (plus inputs),
//! independent of the static 4096-byte resource budget. Disconnect drops worker
//! channels without waiting on the window thread; late replies cannot enter a
//! new session. The sandbox never receives world/inventory references or native
//! networking handles.
//!
//! Unsupported: HTML/CSS, arbitrary script-created widget trees, animations and
//! hot reload. Without the explicit capability, event IDs remain inert.
//! `fixtures/packages/uidemo` is the sample used by ui-preview and loopback tests.
mod draw;
mod egui_view;
mod events;
mod raster;
mod schema;
mod session;

use std::collections::BTreeMap;

use crate::server::client_bundle::ClientPackage;
pub(crate) use egui_view::Intent as EguiIntent;
pub(crate) use session::Session;
use {raster::Atlas, schema::*};

pub(super) const ATLAS_SIZE: usize = 1024;
pub(super) const MAX_TEXT: usize = 128;
const INVALID: &str = "invalid, unsupported or oversized package UI";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Debug)]
pub(crate) struct Resources {
    pub(super) pixels: Vec<u8>,
    documents: Vec<Document>,
    fonts: BTreeMap<String, Vec<raster::Glyph>>,
    font_sources: BTreeMap<String, Vec<u8>>,
    images: BTreeMap<String, super::UiRect>,
}

#[derive(Debug)]
struct Document {
    id: String,
    nodes: Vec<Widget>,
    script: Option<std::sync::Arc<crate::client::presentation::Script>>,
}

#[derive(Debug)]
struct Widget {
    id: String,
    parent: Option<usize>,
    kind: Kind,
    style: Style,
    text: String,
    image: Option<String>,
    event: Option<String>,
}

impl Resources {
    pub(crate) fn owns_document(&self, owner: &str) -> bool {
        self.documents
            .iter()
            .any(|document| document.id.split_once(':').map(|v| v.0) == Some(owner))
    }
    pub(crate) fn validate_startup(
        &self,
        state: &crate::client::startup::State,
    ) -> std::result::Result<(), String> {
        for key in state.texts.keys() {
            if !self.documents.iter().any(|document| {
                document.nodes.iter().any(|node| {
                    node.id == *key && matches!(node.kind, Kind::Label | Kind::Button | Kind::Input)
                })
            }) {
                return Err(format!("client startup {key}: unknown text target"));
            }
        }
        for key in state.states.keys() {
            if !self.documents.iter().any(|document| document.id == *key) {
                return Err(format!("client startup {key}: unknown document"));
            }
        }
        Ok(())
    }
    pub(crate) fn compile(packages: &BTreeMap<String, ClientPackage>) -> Result<Option<Self>> {
        if packages.values().all(|p| p.ui_assets.is_empty()) {
            return Ok(None);
        }
        let mut atlas = Atlas::new();
        let mut fonts = BTreeMap::new();
        let mut font_sources = BTreeMap::new();
        let mut images = BTreeMap::new();
        let mut styles = BTreeMap::new();
        let mut documents = Vec::new();
        let mut image_pixels = 0;
        // The bundle already bounds assets to 256 total and bytes to 4 MiB.
        // All derived resource limits here are global, not per package.
        for (owner, package) in packages {
            for (local, (kind, bytes)) in &package.ui_assets {
                let id = format!("{owner}:{local}");
                match kind {
                    2 => {} // Resolve after all resources exist, in canonical order.
                    3 => {
                        if styles.len() == 64 {
                            return Err(INVALID);
                        }
                        let style: Style = json(bytes)?;
                        style.validate(owner)?;
                        styles.insert(id, style);
                    }
                    4 => {
                        if fonts.len() == 4 {
                            return Err(INVALID);
                        }
                        fonts.insert(id.clone(), atlas.font(bytes)?);
                        font_sources.insert(id, bytes.clone());
                    }
                    5 => {
                        if images.len() == 16 {
                            return Err(INVALID);
                        }
                        images.insert(id, atlas.image(bytes, &mut image_pixels)?);
                    }
                    _ => return Err(INVALID),
                }
            }
        }
        for style in styles.values() {
            if style
                .font
                .as_ref()
                .is_some_and(|font| !fonts.contains_key(font))
            {
                return Err(INVALID);
            }
        }
        let mut nodes = 0;
        let mut text_bytes = 0;
        for (owner, package) in packages {
            for (local, (kind, bytes)) in &package.ui_assets {
                if *kind != 2 {
                    continue;
                }
                if documents.len() == 8 {
                    return Err(INVALID);
                }
                let raw: RawDocument = json(bytes)?;
                let document = raw.resolve(owner, local, package, &styles, &images)?;
                nodes += document.nodes.len();
                // Reserve the full capacity of each input, not just initial text.
                text_bytes += document
                    .nodes
                    .iter()
                    .map(|n| {
                        if n.kind == Kind::Input {
                            MAX_TEXT
                        } else {
                            n.text.len()
                        }
                    })
                    .sum::<usize>();
                if nodes > 256 || text_bytes > 4096 {
                    return Err(INVALID);
                }
                documents.push(document);
            }
        }
        if documents.is_empty() {
            return Err(INVALID);
        }
        Ok(Some(Self {
            pixels: atlas.pixels,
            documents,
            fonts,
            font_sources,
            images,
        }))
    }
}

fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > 16 * 1024 {
        return Err(INVALID);
    }
    serde_json::from_slice(bytes).map_err(|_| INVALID)
}

fn identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}

fn owned(owner: &str, reference: &str) -> bool {
    reference
        .split_once(':')
        .is_some_and(|(package, local)| package == owner && identifier(local))
}

#[cfg(test)]
mod tests;
