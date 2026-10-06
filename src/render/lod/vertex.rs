//! Compact distant geometry: exact positions and voxel light, quantized linear color.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pub position: [f32; 3],
    color: [u8; 4],
    surface: u32,
}
impl Vertex {
    pub(super) fn material(mut self, surface: super::surface::Surface) -> Self {
        self.color[3] = (surface.color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        self.surface |= (u32::from(surface.fluid) << 11)
            | (u32::from(surface.cutout) << 12)
            | (surface.layer.map_or(0, |layer| layer + 1) << 13)
            | (u32::from(surface.layer.is_some() && !surface.sample_texture) << 31);
        self
    }
    pub(super) fn new(
        position: [f32; 3],
        axis: usize,
        side: i32,
        color: [f32; 3],
        sky: u8,
        glow: u8,
    ) -> Self {
        Self {
            position,
            color: [color[0], color[1], color[2], 1.0]
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
            surface: (axis as u32 * 2 + u32::from(side > 0))
                | (u32::from(sky) << 3)
                | (u32::from(glow) << 7),
        }
    }
    pub(super) fn ray_surface(self) -> super::ray::Appearance {
        let encoded = (self.surface >> 13) & 0x3ffff;
        super::ray::Appearance {
            packed: self.surface,
            layer: encoded.saturating_sub(1),
            color: self.color.map(|c| f32::from(c) / 255.0),
            textured: encoded != 0 && self.surface & 0x80000000 == 0,
        }
    }
    #[cfg(test)]
    pub(super) fn unpack(self) -> [f32; 11] {
        let mut out = [0.0; 11];
        out[..3].copy_from_slice(&self.position);
        out[3 + (self.surface & 7) as usize / 2] = if self.surface & 1 == 0 { -1.0 } else { 1.0 };
        for (i, c) in self.color[..3].iter().enumerate() {
            out[6 + i] = f32::from(*c) / 255.0;
        }
        out[9] = ((self.surface >> 3) & 15) as f32 / 15.0;
        out[10] = ((self.surface >> 7) & 15) as f32 / 15.0;
        out
    }
}
#[cfg(test)]
mod tests;
