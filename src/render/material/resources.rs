//! Admission before material pixels, mipmaps or GPU arrays are allocated.
use super::{TEXTURE_MIPS, TEXTURE_SIZE};

// Preserve 512 admitted catalog layers at 256px across the three mipmapped
// arrays. This is a ceiling; native worlds allocate only their actual layers.
pub(crate) const MAX_ARRAY_BYTES: u64 = 512 * 1024 * 1024;
const fn bytes_per_layer() -> u64 {
    let mut bytes = 0;
    let mut level = 0;
    while level < TEXTURE_MIPS {
        let size = TEXTURE_SIZE >> level;
        bytes += size as u64 * size as u64 * 4;
        level += 1;
    }
    bytes
}
// Albedo, normal and specular arrays all have a full mip chain.
pub(crate) const BYTES_PER_LAYER: u64 = bytes_per_layer() * 3;
pub(crate) const MAX_ARRAY_LAYERS: u32 = (MAX_ARRAY_BYTES / BYTES_PER_LAYER) as u32;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ArrayUsage {
    pub layers: u32,
    pub mip_bytes: u64,
}

pub(crate) fn validate(layers: usize, device_layers: u32) -> Result<ArrayUsage, String> {
    let layers = u32::try_from(layers).map_err(|_| {
        format!("material texture array layers: attempted {layers}; maximum {MAX_ARRAY_LAYERS}")
    })?;
    let mip_bytes = u64::from(layers) * BYTES_PER_LAYER;
    if mip_bytes > MAX_ARRAY_BYTES {
        return Err(format!(
            "material texture array bytes/installation: attempted {mip_bytes}; maximum {MAX_ARRAY_BYTES} ({layers} layers at {TEXTURE_SIZE}x{TEXTURE_SIZE}, {TEXTURE_MIPS} mip levels)"
        ));
    }
    if layers == 0 || layers > device_layers {
        return Err(format!(
            "material texture array layers/device: attempted {layers}; maximum {device_layers}"
        ));
    }
    Ok(ArrayUsage { layers, mip_bytes })
}

pub(crate) fn required_limits(
    adapter: wgpu::Limits,
    layers: usize,
) -> Result<wgpu::Limits, String> {
    validate(layers, adapter.max_texture_array_layers)?;
    if adapter.max_texture_dimension_2d < TEXTURE_SIZE {
        return Err(format!(
            "material texture dimensions/device: attempted {TEXTURE_SIZE}; maximum {}",
            adapter.max_texture_dimension_2d
        ));
    }
    Ok(wgpu::Limits {
        max_texture_array_layers: adapter.max_texture_array_layers.min(MAX_ARRAY_LAYERS),
        ..wgpu::Limits::default()
    })
}

#[cfg(test)]
mod tests;
