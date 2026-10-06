//! Static appearance metadata; fine LODs reuse the admitted near texture array.
use super::FaceColors;
use crate::{
    content::{self, Catalog},
    world::BlockId,
};
#[derive(Clone, Copy)]
pub(super) struct Surface {
    pub color: [f32; 4],
    pub layer: Option<u32>,
    pub sample_texture: bool,
    pub fluid: bool,
    pub cutout: bool,
}
impl Surface {
    pub fn new(
        catalog: &Catalog,
        colors: &FaceColors,
        state: BlockId,
        axis: usize,
        side: i32,
        level: u8,
    ) -> Self {
        let flags = catalog.block_flags(state);
        let fluid = flags & content::FLUID != 0;
        let color = if fluid {
            let mut rgba = catalog
                .block(state)
                .map_or(bloxgloom_host_api::content::DEFAULT_FLUID_SWATCH, |b| {
                    b.swatch
                });
            for c in &mut rgba[..3] {
                *c = if *c <= 0.04045 {
                    *c / 12.92
                } else {
                    ((*c + 0.055) / 1.055).powf(2.4)
                };
            }
            rgba
        } else {
            let c = colors.face(catalog, state, axis, side);
            [c[0], c[1], c[2], 1.0]
        };
        Self {
            color,
            fluid,
            cutout: flags & content::CUTOUT != 0,
            sample_texture: level <= 2,
            layer: ((level <= 2 || super::super::bsl_reference::enabled()) && !fluid)
                .then(|| super::super::material::material_layer_for(catalog, state, axis, side)),
        }
    }
}
pub(super) fn occludes(catalog: &Catalog, source: BlockId, other: BlockId) -> bool {
    catalog.block_flags(other) & content::FLUID == 0
        || catalog.block_flags(source) & content::FLUID != 0
}
