//! Additional surface semantics in the existing 96-byte triangle packet.
//! Integer payloads preserve all RGBA8 bits without NaN float encodings.
use super::Triangle;

pub(crate) const LOD: u32 = 1;
pub(crate) const WATER: u32 = 2;
pub(crate) const COARSE_COLOR: u32 = 4;
pub(crate) const TEXTURE: u32 = 8;
pub(crate) const NO_WIND: u32 = 16;
pub(crate) const GLOW_SHIFT: u32 = 8;

impl Triangle {
    pub(crate) fn with_surface(mut self, color: [f32; 4], flags: u32) -> Self {
        self.surface_color =
            u32::from_le_bytes(color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
        self.surface_flags = flags;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn opaque_white_and_independent_flags_keep_bits_without_expanding_triangles() {
        let mut triangle = Triangle::zeroed();
        triangle.uv_c = [-2.0, 3.5];
        let packed = triangle.with_surface([1.0; 4], LOD | COARSE_COLOR | NO_WIND);
        assert_eq!(std::mem::size_of::<Triangle>(), 96);
        assert_eq!(std::mem::offset_of!(Triangle, surface_color), 72);
        assert_eq!(std::mem::offset_of!(Triangle, surface_flags), 76);
        assert_eq!(packed.surface_color, u32::MAX);
        assert_eq!(packed.surface_flags, 21);
        assert_eq!(packed.uv_c, [-2.0, 3.5]);
        let bytes = bytemuck::bytes_of(&packed);
        let decoded: &Triangle = bytemuck::from_bytes(bytes);
        assert_eq!(decoded.surface_color, u32::MAX);
        assert_eq!(decoded.surface_flags & WATER, 0);
        assert_eq!(decoded.surface_flags & TEXTURE, 0);
        let water = packed.with_surface([-1.0, 0.5, 2.0, 0.7], WATER | NO_WIND);
        assert_eq!(water.surface_color.to_le_bytes(), [0, 128, 255, 179]);
        assert_eq!(water.surface_flags, 18);
    }
}
