use super::*;

#[test]
fn counts_every_mipmap_and_enforces_the_array_byte_boundary() {
    assert_eq!(BYTES_PER_LAYER, 349_524 * 3);
    let admitted = validate(MAX_ARRAY_LAYERS as usize, u32::MAX).unwrap();
    assert!(admitted.mip_bytes <= MAX_ARRAY_BYTES);
    let error = validate(MAX_ARRAY_LAYERS as usize + 1, u32::MAX).unwrap_err();
    assert!(error.contains("bytes/installation"));
    assert!(error.contains(&format!("maximum {MAX_ARRAY_BYTES}")));
    assert!(validate(usize::MAX, u32::MAX).is_err());
}

#[test]
fn requests_enough_device_layers_for_package_textures_and_native_materials() {
    let layers = MAX_ARRAY_LAYERS as usize;
    assert!(layers > wgpu::Limits::default().max_texture_array_layers as usize);
    let adapter = wgpu::Limits {
        max_texture_array_layers: 2048,
        ..wgpu::Limits::default()
    };
    let requested = required_limits(adapter, layers).unwrap();
    assert!(requested.max_texture_array_layers as usize >= layers);
    assert!(requested.max_texture_array_layers <= MAX_ARRAY_LAYERS);
    assert!(validate(layers, requested.max_texture_array_layers).is_ok());
    let low_adapter = wgpu::Limits {
        max_texture_array_layers: 256,
        ..wgpu::Limits::default()
    };
    let error = required_limits(low_adapter, layers).unwrap_err();
    assert!(error.contains("layers/device"));
    assert!(error.contains("maximum 256"));
}

#[test]
fn rejects_unsupported_dimensions_before_resource_creation() {
    let adapter = wgpu::Limits {
        max_texture_dimension_2d: 64,
        ..wgpu::Limits::default()
    };
    assert!(
        required_limits(adapter, 1)
            .unwrap_err()
            .contains("dimensions/device: attempted 256; maximum 64")
    );
}

#[test]
fn target_package_texture_count_builds_a_valid_gpu_material_array() {
    let mut catalog = crate::content::Catalog::builtins();
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[120, 180, 60, 255].repeat(256))
            .unwrap();
    }
    let png: std::borrow::Cow<'static, [u8]> = std::borrow::Cow::Owned(png_bytes);
    let target = MAX_ARRAY_LAYERS as usize;
    let extra = target - super::super::texture_layers_for(&catalog) as usize;
    for i in 0..extra {
        catalog
            .public_texture(&bloxgloom_host_api::content::Texture {
                key: format!("capacity:t{i}"),
                png: png.clone(),
                stitch_edges: true,
                stitch_vertical: true,
                alpha_cutout: false,
                emission_strength: 0.0,
                foliage: Default::default(),
            })
            .unwrap();
    }
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        apply_limit_buckets: false,
        ..Default::default()
    })) else {
        eprintln!("material capacity GPU test skipped: no headless adapter");
        return;
    };
    let required_limits = match required_limits(
        adapter.limits(),
        super::super::texture_layers_for(&catalog) as usize,
    ) {
        Ok(limits) => limits,
        Err(error) => {
            eprintln!(
                "material capacity GPU test skipped: adapter admission rejected target: {error}"
            );
            return;
        }
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits,
        ..Default::default()
    }))
    .unwrap();
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipelines = crate::render::pipeline::create_voxel_pipeline_with_catalog(
        &device,
        &queue,
        crate::render::post::HDR_FORMAT,
        &catalog,
    )
    .unwrap();
    assert!(pollster::block_on(error_scope.pop()).is_none());
    drop(pipelines);
}
