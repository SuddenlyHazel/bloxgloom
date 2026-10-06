use super::*;
use sha2::{Digest, Sha256};
use std::sync::mpsc;
pub(super) struct Ready {
    pub source: Option<wgpu::Buffer>,
    pub offsets: HashMap<usize, u32>,
    pub assets: Vec<Arc<DynamicAsset>>,
}
pub(super) struct Uploader {
    send: mpsc::Sender<Vec<Arc<DynamicAsset>>>,
    receive: mpsc::Receiver<Ready>,
    requested: Vec<usize>,
}
impl Uploader {
    pub fn new(device: wgpu::Device) -> Self {
        let (send, receive) = mpsc::channel::<Vec<Arc<DynamicAsset>>>();
        let (publish, ready) = mpsc::channel();
        std::thread::Builder::new()
            .name("ray-dynamic-upload".into())
            .spawn(move || {
                let mut words = Vec::new();
                let mut images = HashMap::new();
                let mut offsets = HashMap::new();
                let mut admitted = Vec::new();
                let limit = device
                    .limits()
                    .max_storage_buffer_binding_size
                    .min(device.limits().max_buffer_size);
                while let Ok(mut assets) = receive.recv() {
                    for latest in receive.try_iter() {
                        assets = latest;
                    }
                    for asset in &assets {
                        let key = Arc::as_ptr(asset) as usize;
                        if let std::collections::hash_map::Entry::Vacant(entry) = offsets.entry(key)
                        {
                            entry.insert(pack_asset(&mut words, asset, &mut images));
                            admitted.push(asset.clone());
                        }
                    }
                    // Cached but departed artwork must not prevent recovery after
                    // an unusually large model is removed from the current scene.
                    if words.len() as u64 * 4 > limit {
                        words.clear();
                        images.clear();
                        offsets.clear();
                        admitted.clear();
                        for asset in assets {
                            let key = Arc::as_ptr(&asset) as usize;
                            offsets.insert(key, pack_asset(&mut words, &asset, &mut images));
                            admitted.push(asset);
                        }
                    }
                    let source = if words.len() as u64 * 4 <= limit {
                        Some(buffer(
                            &device,
                            "native dynamic assets",
                            &words,
                            wgpu::BufferUsages::STORAGE,
                        ))
                    } else {
                        tracing::warn!("native dynamic artwork exceeds adapter storage limit");
                        None
                    };
                    if publish
                        .send(Ready {
                            source,
                            offsets: offsets.clone(),
                            assets: admitted.clone(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("dynamic ray upload worker");
        Self {
            send,
            receive: ready,
            requested: Vec::new(),
        }
    }
    pub fn request(&mut self, assets: impl IntoIterator<Item = Arc<DynamicAsset>>) {
        let mut unique = std::collections::BTreeMap::new();
        for asset in assets {
            unique.insert(Arc::as_ptr(&asset) as usize, asset);
        }
        let keys = unique.keys().copied().collect::<Vec<_>>();
        if keys == self.requested {
            return;
        }
        self.requested = keys;
        let _ = self.send.send(unique.into_values().collect());
    }
    pub fn poll(&self) -> Option<Ready> {
        self.receive.try_iter().last()
    }
}
fn pack_asset(
    words: &mut Vec<u32>,
    asset: &DynamicAsset,
    image_cache: &mut HashMap<([u8; 32], u32, u32), u32>,
) -> u32 {
    let base = words.len();
    words.resize(base + 8, 0);
    let vertices = words.len();
    for v in &asset.vertices {
        words.extend(v.position.map(f32::to_bits));
        words.push(v.part);
        words.extend(v.normal.map(f32::to_bits));
        words.push(0);
        words.extend(v.uv.map(f32::to_bits));
        words.extend(v.joints);
        words.extend(v.weights.map(f32::to_bits));
        words.extend([0; 2]);
    }
    let triangles = words.len();
    for t in &asset.triangles {
        words.extend(t.indices);
        words.extend([t.material, t.part, 0, 0, 0]);
    }
    let materials = words.len();
    words.resize(materials + asset.materials.len() * 16, 0);
    let mut images = Vec::new();
    for image in &asset.images {
        let key = (
            <[u8; 32]>::from(Sha256::digest(&image.rgba)),
            image.width,
            image.height,
        );
        let pixels = *image_cache.entry(key).or_insert_with(|| {
            let offset = words.len() as u32;
            words.extend(
                image
                    .rgba
                    .chunks_exact(4)
                    .map(|p| u32::from_le_bytes(p.try_into().unwrap())),
            );
            offset
        });
        images.push((pixels, image.width, image.height));
    }
    for (index, m) in asset.materials.iter().enumerate() {
        let at = materials + index * 16;
        let (pixels, width, height) = m.image.map_or((0, 0, 0), |i| images[i]);
        words[at..at + 16].copy_from_slice(&[
            m.kind,
            pixels,
            width,
            height,
            m.base[0].to_bits(),
            m.base[1].to_bits(),
            m.base[2].to_bits(),
            m.base[3].to_bits(),
            m.wrap[0],
            m.wrap[1],
            m.alpha_cutoff.to_bits(),
            u32::from(m.double_sided),
            m.surface,
            m.group,
            m.catalog_layer,
            0,
        ]);
    }
    words[base..base + 8].copy_from_slice(&[
        vertices as u32,
        triangles as u32,
        materials as u32,
        asset.triangles.len() as u32,
        asset.nodes.len() as u32,
        0,
        0,
        0,
    ]);
    base as u32
}
