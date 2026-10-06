//! Compact distant geometry: exact positions/light, opaque UNORM and fluid RGB9E5.
mod fluid_color;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pub position: [f32; 3],
    color: [u8; 4],
    surface: u32,
}
impl Vertex {
    pub(super) fn material(mut self, surface: super::surface::Surface) -> Self {
        let alpha = (surface.color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        if surface.fluid {
            // Fluid tints are dark in linear space. UNORM8 changes the ocean's
            // red by twelve percent; darker tints can lose a channel entirely.
            // RGB9E5 uses the same four bytes; fluids need no material layer.
            self.color = fluid_color::encode(surface.color[..3].try_into().unwrap()).to_le_bytes();
            self.surface |= u32::from(alpha) << 13;
        } else {
            self.color[3] = alpha;
        }
        self.surface |= (u32::from(surface.fluid) << 11)
            | (u32::from(surface.cutout) << 12)
            | (surface
                .layer
                .filter(|_| !surface.fluid)
                .map_or(0, |layer| layer + 1)
                << 13)
            | (u32::from(!surface.fluid && surface.layer.is_some() && !surface.sample_texture)
                << 31);
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
        let fluid = self.surface & (1 << 11) != 0;
        let encoded = if fluid {
            0
        } else {
            (self.surface >> 13) & 0x3ffff
        };
        super::ray::Appearance {
            packed: self.surface,
            layer: encoded.saturating_sub(1),
            color: self.decoded_color(),
            textured: encoded != 0 && self.surface & 0x80000000 == 0,
            reconstructed: self.surface & ((1 << 11) | (1 << 12)) == 0 && self.color[3] == 254,
        }
    }
    pub(super) fn mark_reconstructed(&mut self) {
        // Opaque summaries have alpha one. Reserve this otherwise-unused byte
        // value for geometric-normal reconstruction, preserving vertex stride.
        self.color[3] = 254;
    }
    fn decoded_color(self) -> [f32; 4] {
        if self.surface & (1 << 11) != 0 {
            let rgb = fluid_color::decode(u32::from_le_bytes(self.color));
            [
                rgb[0],
                rgb[1],
                rgb[2],
                ((self.surface >> 13) & 255) as f32 / 255.0,
            ]
        } else {
            let mut color = self.color.map(|c| f32::from(c) / 255.0);
            if self.surface & (1 << 12) == 0 && self.color[3] == 254 {
                color[3] = 1.0;
            }
            color
        }
    }
    #[cfg(test)]
    pub(super) fn unpack(self) -> [f32; 11] {
        let mut out = [0.0; 11];
        out[..3].copy_from_slice(&self.position);
        out[3 + (self.surface & 7) as usize / 2] = if self.surface & 1 == 0 { -1.0 } else { 1.0 };
        out[6..9].copy_from_slice(&self.decoded_color()[..3]);
        out[9] = ((self.surface >> 3) & 15) as f32 / 15.0;
        out[10] = ((self.surface >> 7) & 15) as f32 / 15.0;
        out
    }
}
#[cfg(test)]
mod tests;
