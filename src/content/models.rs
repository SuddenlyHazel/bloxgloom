//! Immutable startup-prepared models and their negotiated identities. Admission
//! estimates decoded allocation before PNG/geometry decoding, including repeated
//! mesh instances and animation sampler references.
use super::*;
use crate::render::model_asset::{Controls, Model, Vertex};
use bloxgloom_host_api::{RegistrationError as Error, model::ModelAsset};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, Weak};
static PREPARED: Mutex<std::collections::BTreeMap<[u8; 32], Weak<Prepared>>> =
    Mutex::new(std::collections::BTreeMap::new());

pub(crate) const MAX_MODELS: usize = 128;
const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;
const MAX_PROCESS_BYTES: usize = 256 * 1024 * 1024;
static MEMORY: Mutex<usize> = Mutex::new(0);

struct Reservation(usize);
impl Drop for Reservation {
    fn drop(&mut self) {
        *MEMORY.lock().unwrap() -= self.0;
    }
}
pub(crate) struct Prepared {
    pub model: Arc<Model>,
    pub scale: f32,
    pub fingerprint: u64,
    visual_schema: bloxgloom_host_api::entity::VisualSchema,
    bytes: usize,
    _reservation: Reservation,
}
impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedModel")
            .field("bytes", &self.bytes)
            .field("scale", &self.scale)
            .finish()
    }
}
impl Prepared {
    pub(crate) fn decoded_bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn visual_schema(&self) -> &bloxgloom_host_api::entity::VisualSchema {
        &self.visual_schema
    }
    pub(crate) fn schema(&self) -> bloxgloom_host_api::entity::VisualSchema {
        self.visual_schema.clone()
    }
}
fn schema(model: &Model) -> bloxgloom_host_api::entity::VisualSchema {
    bloxgloom_host_api::entity::VisualSchema {
        clips: model.clips.iter().map(|c| c.name.clone()).collect(),
        clip_loops: model
            .clips
            .iter()
            .map(|c| model.controls.loops.get(&c.name).copied().unwrap_or(false))
            .collect(),
        variants: model
            .controls
            .variants
            .iter()
            .map(|v| {
                (
                    v.name.clone(),
                    v.options.iter().map(|o| o.name.clone()).collect(),
                )
            })
            .collect(),
        layers: model
            .controls
            .layers
            .iter()
            .map(|l| l.name.clone())
            .collect(),
        tints: model
            .controls
            .tints
            .iter()
            .map(|t| t.name.clone())
            .collect(),
    }
}

pub(crate) fn prepare(asset: &ModelAsset) -> Result<Arc<Prepared>, Error> {
    asset.validate()?;
    let mut hash = Sha256::new();
    for bytes in [
        &asset.glb[..],
        &asset.controls[..],
        &asset.scale.to_le_bytes()[..],
    ] {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    let key: [u8; 32] = hash.finalize().into();
    let mut cache = PREPARED.lock().unwrap();
    cache.retain(|_, model| model.strong_count() != 0);
    if let Some(model) = cache.get(&key).and_then(Weak::upgrade) {
        return Ok(model);
    }
    let fail = |message: String| Error(format!("{}: {message}", asset.key));
    let controls = if asset.controls.is_empty() {
        Controls::default()
    } else {
        serde_json::from_slice::<Controls>(&asset.controls)
            .map_err(|e| fail(format!("invalid model controls: {e}")))?
    };
    if controls.variants.len() > 16 || controls.layers.len() > 32 || controls.tints.len() > 16 {
        return Err(fail(
            "model visual schema exceeds 16 variants, 32 layers or 16 tints".into(),
        ));
    }
    let bytes = estimate(&asset.glb).map_err(fail)?;
    {
        let mut used = MEMORY.lock().unwrap();
        if bytes > MAX_DECODED_BYTES || used.saturating_add(bytes) > MAX_PROCESS_BYTES {
            return Err(fail("decoded model bytes admission exhausted".into()));
        }
        *used += bytes;
    }
    let reservation = Reservation(bytes);
    let model = Model::from_glb(&asset.glb, controls).map_err(fail)?;
    if model.clips.len() > 256 || model.clips.iter().any(|c| c.name.len() > 96) {
        return Err(fail("model clip names must be at most 96 bytes".into()));
    }
    let mut fingerprint = 0xcbf2_9ce4_8422_2325;
    hash_bytes(&mut fingerprint, b"native-glb-v1");
    hash_bytes(&mut fingerprint, &asset.glb);
    hash_bytes(&mut fingerprint, &asset.controls);
    hash_bytes(&mut fingerprint, &asset.scale.to_le_bytes());
    let prepared = Arc::new(Prepared {
        visual_schema: schema(&model),
        model: Arc::new(model),
        scale: asset.scale,
        fingerprint,
        bytes,
        _reservation: reservation,
    });
    cache.insert(key, Arc::downgrade(&prepared));
    Ok(prepared)
}

fn estimate(bytes: &[u8]) -> Result<usize, String> {
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| format!("invalid GLB: {e}"))?;
    let blob = g
        .blob
        .as_deref()
        .ok_or("GLB needs embedded binary buffer")?;
    let mut decoded = 128 * 1024_usize;
    for image in g.images() {
        let gltf::image::Source::View {
            view,
            mime_type: "image/png",
        } = image.source()
        else {
            return Err("model needs embedded PNGs".into());
        };
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or("image view overflow")?;
        let png = blob
            .get(view.offset()..end)
            .ok_or("image view out of bounds")?;
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder
            .read_info()
            .map_err(|e| format!("invalid model PNG: {e}"))?;
        let (width, height) = (reader.info().width, reader.info().height);
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err("model texture dimensions must be 1..2048".into());
        }
        // RGBA output and decode scratch can coexist; reserve both conservatively.
        decoded = decoded.saturating_add(width as usize * height as usize * 8);
    }
    let mut vertices = 0_usize;
    for node in g.nodes() {
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let positions = primitive
                    .get(&gltf::Semantic::Positions)
                    .ok_or("mesh needs positions")?
                    .count();
                vertices = vertices.saturating_add(positions);
                decoded = decoded
                    .saturating_add(positions.saturating_mul(std::mem::size_of::<Vertex>() + 32));
                decoded = decoded.saturating_add(
                    primitive
                        .indices()
                        .map_or(positions, |i| i.count())
                        .saturating_mul(4),
                );
            }
        }
    }
    if vertices > 196608 {
        return Err("model vertex admission limit exceeded".into());
    }
    for animation in g.animations() {
        for channel in animation.channels() {
            let sampler = channel.sampler();
            decoded = decoded.saturating_add(sampler.input().count().saturating_mul(4));
            decoded = decoded.saturating_add(sampler.output().count().saturating_mul(16));
        }
    }
    if decoded > MAX_DECODED_BYTES {
        return Err("decoded model bytes exceed 128 MiB".into());
    }
    Ok(decoded)
}

impl Catalog {
    pub(crate) fn register_model_asset(&mut self, asset: &ModelAsset) -> Result<(), Error> {
        self.register_prepared_model(asset.key.clone(), prepare(asset)?)
    }
    pub(crate) fn register_prepared_model(
        &mut self,
        key: String,
        model: Arc<Prepared>,
    ) -> Result<(), Error> {
        let id = self.models.keys().next_back().map_or(0, |id| id + 1);
        self.bind_model(id, key, model)
    }
    pub(super) fn bind_model(
        &mut self,
        id: u32,
        key: String,
        model: Arc<Prepared>,
    ) -> Result<(), Error> {
        let bytes = self.models().map(|(_, m)| m.bytes).sum::<usize>();
        if !valid_key(&key)
            || id >= MAX_ASSIGNED_ID
            || self.models.len() >= MAX_MODELS
            || self.models.contains_key(&id)
            || self.model_keys.contains_key(&key)
            || bytes.saturating_add(model.bytes) > MAX_DECODED_BYTES
        {
            return Err(Error(
                "duplicate model or model catalog admission exhausted".into(),
            ));
        }
        self.model_keys.insert(key.clone(), id);
        self.models.insert(id, (key, model));
        Ok(())
    }
    pub(crate) fn model_by_key(&self, key: &str) -> Option<&Arc<Prepared>> {
        let id = self.model_keys.get(key)?;
        Some(&self.models.get(id)?.1)
    }
    pub(crate) fn models(&self) -> impl Iterator<Item = (u32, &Arc<Prepared>)> {
        self.models.iter().map(|(id, (_, model))| (*id, model))
    }
}

#[cfg(test)]
mod tests;
