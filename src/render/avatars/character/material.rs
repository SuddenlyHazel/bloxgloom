//! Native-size pixel atlases. Character artwork never enters terrain resampling.
use wgpu::util::DeviceExt;

pub(super) fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("builtin character PNG header");
    let mut bytes = vec![0; reader.output_buffer_size().expect("bounded builtin PNG")];
    let info = reader
        .next_frame(&mut bytes)
        .expect("builtin character PNG pixels");
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => bytes[..info.buffer_size()]
            .chunks_exact(3)
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
        _ => panic!("builtin character PNG must be RGB/RGBA"),
    };
    (info.width, info.height, rgba)
}

pub(super) fn array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[&[u8]],
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::TextureView {
    assert!(!layers.is_empty() && layers.len() <= 64);
    let (width, height, _) = decode(layers[0]);
    assert!(width <= 512 && height <= 512);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize * layers.len());
    for layer in layers {
        let (layer_width, layer_height, rgba) = decode(layer);
        assert_eq!(
            (layer_width, layer_height),
            (width, height),
            "face features must retain 32-pixel UVs"
        );
        pixels.extend(rgba);
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
            format,
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
