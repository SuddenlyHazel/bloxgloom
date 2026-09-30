//! Verified package widgets, frozen resources and bounded local UI sessions.
//! Version 1 retains the original fixed five-kind schema; version 2 adds typed
//! controls, scroll/table containers, dynamic descendants and declared bindings.
//! Callback evaluation and command decoding happen on presentation workers.
//! Runtime candidate trees validate atomically against frozen resources before
//! publication. Drawing never evaluates source or performs I/O. Stable IDs retain
//! edits/focus; actual egui intents carry tree generations to reject stale input.
//! See docs/modding/DYNAMIC-UI.md for limits, authority and event semantics.
pub(crate) mod bindings;
mod controls;
mod draw;
mod dynamic;
mod egui_view;
mod events;
mod raster;
mod schema;
mod session;

use std::collections::BTreeMap;

use crate::server::client_bundle::ClientPackage;
pub(crate) use egui_view::Intent as EguiIntent;
pub(crate) use schema::RawNode;
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
    styles: BTreeMap<String, Style>,
}

#[derive(Clone, Debug)]
pub(crate) struct Document {
    id: String,
    version: u8,
    nodes: Vec<Widget>,
    bindings: Vec<bindings::Binding>,
    script: Option<std::sync::Arc<crate::client::presentation::Script>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Widget {
    id: String,
    parent: Option<usize>,
    kind: Kind,
    style: Style,
    text: String,
    image: Option<String>,
    event: Option<String>,
    control: controls::Control,
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
                document
                    .nodes
                    .iter()
                    .any(|node| node.id == *key && node.kind.textual())
            }) {
                return Err(format!("client startup {key}: unknown text target"));
            }
        }
        for key in state.states.keys() {
            if !self.documents.iter().any(|document| document.id == *key) {
                return Err(format!("client startup {key}: unknown document"));
            }
        }
        for document in &self.documents {
            let bytes = document
                .nodes
                .iter()
                .map(|node| {
                    let text = state.texts.get(&node.id).unwrap_or(&node.text).len();
                    text + if node.kind.input() {
                        controls::Control::limit(node.kind)
                    } else {
                        0
                    } + match &node.control {
                        controls::Control::Select { options, .. } => options
                            .iter()
                            .map(|option| option.key.len() + option.label.len())
                            .sum::<usize>(),
                        _ => 0,
                    }
                })
                .sum::<usize>();
            if bytes > 32 * 1024 {
                return Err(format!(
                    "client startup {}: runtime UI text budget exceeded",
                    document.id
                ));
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
        let mut version2 = false;
        let mut binding_keys = std::collections::BTreeSet::new();
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
                version2 |= document.version == 2;
                for binding in &document.bindings {
                    if !binding_keys.insert(binding.key.clone()) || binding_keys.len() > 64 {
                        return Err(INVALID);
                    }
                }
                nodes += document.nodes.len();
                // Reserve the full capacity of each input, not just initial text.
                text_bytes += document
                    .nodes
                    .iter()
                    .map(|n| {
                        let text = if n.kind.input() {
                            controls::Control::limit(n.kind)
                                + if document.version == 2 {
                                    n.text.len()
                                } else {
                                    0
                                }
                        } else {
                            n.text.len()
                        };
                        text + match &n.control {
                            controls::Control::Select { options, .. } => options
                                .iter()
                                .map(|option| option.key.len() + option.label.len())
                                .sum::<usize>(),
                            _ => 0,
                        }
                    })
                    .sum::<usize>();
                documents.push(document);
            }
        }
        if documents.is_empty()
            || nodes > if version2 { 1024 } else { 256 }
            || text_bytes > if version2 { 32768 } else { 4096 }
        {
            return Err(INVALID);
        }
        Ok(Some(Self {
            pixels: atlas.pixels,
            documents,
            fonts,
            font_sources,
            images,
            styles,
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
