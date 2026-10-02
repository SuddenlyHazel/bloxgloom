//! Upload decoded embedded GLB images at their native dimensions.
use crate::render::model_asset::Image;
use wgpu::util::DeviceExt;
pub(super) fn array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[Image],
    label: &str,
) -> wgpu::TextureView {
    let (width, height) = (layers[0].width, layers[0].height);
    let mut pixels = Vec::new();
    for image in layers {
        assert_eq!(
            (image.width, image.height),
            (width, height),
            "GLB player atlas dimensions differ"
        );
        pixels.extend_from_slice(&image.rgba);
    }
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &pixels,
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
