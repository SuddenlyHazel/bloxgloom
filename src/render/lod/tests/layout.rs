//! Parser checks cannot detect missing material/shadow bindings in GPU layouts.
#[test]
fn production_distant_terrain_and_water_layouts_accept_reference_receivers() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let catalog = crate::content::catalog();
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: crate::render::material_device_limits(
                adapter.limits(),
                crate::render::material_texture_layers(catalog) as usize,
            )
            .unwrap(),
            ..Default::default()
        }),
    )
    .unwrap();
    let (pipeline, _, _, _, materials) =
        crate::render::create_voxel_pipeline(&device, &queue, crate::render::post::HDR_FORMAT);
    let mut gpu = crate::render::lod::Gpu::new(
        &device,
        crate::render::post::HDR_FORMAT,
        &pipeline,
        &materials,
    );
    let water = crate::render::water::WaterRenderer::new(
        &device,
        &device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: crate::render::daylight::CAMERA_BYTES,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        }),
    );
    gpu.set_reference_water_inputs(&device, water.reference_inputs());
    gpu.set_optical_water_inputs(&device, water.optical_inputs());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[path = "draw_resources.rs"]
mod draw_resources;
