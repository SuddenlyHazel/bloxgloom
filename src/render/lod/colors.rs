use crate::content::Catalog;
/// Texture-derived linear colors. Construct on a preparation/mesh worker.
pub(crate) struct FaceColors {
    layers: Vec<[f32; 3]>,
}
impl FaceColors {
    pub(crate) fn new(catalog: &Catalog) -> Self {
        let pixels = super::super::material::material_tiles_for(catalog);
        let bytes = (super::super::material::TEXTURE_SIZE.pow(2) * 4) as usize;
        let layers = pixels
            .chunks_exact(bytes)
            .map(|layer| {
                let mut sum = [0.0; 3];
                let mut weight = 0.0;
                for px in layer.chunks_exact(4) {
                    let a = px[3] as f32 / 255.0;
                    weight += a;
                    for c in 0..3 {
                        let s = px[c] as f32 / 255.0;
                        sum[c] += a * if s <= 0.04045 {
                            s / 12.92
                        } else {
                            ((s + 0.055) / 1.055).powf(2.4)
                        };
                    }
                }
                sum.map(|s| s / weight.max(1.0))
            })
            .collect();
        Self { layers }
    }
    pub(super) fn face(
        &self,
        catalog: &Catalog,
        state: crate::world::BlockId,
        axis: usize,
        side: i32,
    ) -> [f32; 3] {
        let layer = super::super::material::material_layer_for(catalog, state, axis, side) as usize;
        self.layers.get(layer).copied().unwrap_or([0.3, 0.3, 0.3])
    }
}
