//! Frozen admission for optional fragment-written raw samples and float32 moments.
pub(super) const BYTES_PER_PIXEL: u64 = 48 + 32 + 32;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub(super) fn requested() -> bool {
    std::env::var("BLOXGLOOM_GI_WATER_RECONSTRUCTION").as_deref() == Ok("1")
}

pub(super) fn supported(adapter: &wgpu::Adapter) -> bool {
    let limits = adapter.limits();
    let format = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba32Float);
    adapter
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE)
        && limits.max_storage_textures_per_shader_stage >= 1
        && limits.max_texture_array_layers >= 3
        && limits.max_color_attachment_bytes_per_sample >= 32
        && format.allowed_usages.contains(
            wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
        )
        && format
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::STORAGE_WRITE_ONLY)
}

pub(super) fn fits(device: &wgpu::Device, size: wgpu::Extent3d) -> bool {
    let limits = device.limits();
    size.width <= limits.max_texture_dimension_2d
        && size.height <= limits.max_texture_dimension_2d
        && limits.max_texture_array_layers >= 3
        && limits.max_storage_textures_per_shader_stage >= 1
        && u64::from(size.width) * u64::from(size.height) * BYTES_PER_PIXEL <= MAX_BYTES
}
