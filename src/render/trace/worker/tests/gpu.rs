//! Exercise live-mode preparation without the headless synchronous shortcut.
use super::*;

#[test]
fn gpu_live_preparation_admits_current_scene_and_handles_resize_during_build() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let catalog = crate::content::Catalog::builtins();
    let limits = crate::render::material::resources::required_limits(
        adapter.limits(),
        crate::render::material::texture_layers_for(&catalog) as usize,
    )
    .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .unwrap();
    let (pipeline, _, _, _, _) = crate::render::pipeline::create_voxel_pipeline_with_catalog(
        &device,
        &queue,
        crate::render::post::HDR_FORMAT,
        &catalog,
    )
    .unwrap();
    let storage_limit = device.limits().max_storage_buffer_binding_size;
    let mut trace = crate::render::trace::TraceLighting {
        worker: Worker::new(storage_limit),
        scene: None,
        gpu: None,
        active_revision: 0,
        enabled: true,
        storage_limit,
        profile: None,
        headless: false,
        size: None,
    };
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    trace.set(key, Some(geometry(0.0)));
    assert!(
        !trace.ready(),
        "live mode must wait for prepared GPU objects"
    );
    let size = wgpu::Extent3d {
        width: 64,
        height: 32,
        depth_or_array_layers: 1,
    };
    // This call is intentionally before readiness, as in Renderer::render.
    trace.prepare_gpu(&device, &pipeline, size);
    assert!(trace.worker.gpu_configured);
    assert!(
        trace.gpu.is_none(),
        "requesting preparation must not synchronously build a GPU scene"
    );
    let resized = wgpu::Extent3d {
        width: 128,
        height: 64,
        ..size
    };
    trace.resize(&device, resized);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !trace.ready() {
        trace.install(&device);
        assert!(
            Instant::now() < deadline,
            "prepared live scene never became ready"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let gpu = trace.gpu.as_ref().unwrap();
    let scale = match std::env::var("BLOXGLOOM_GI_SCALE").as_deref() {
        Ok("8") => 8,
        Ok("4") => 4,
        _ => 2,
    };
    assert_eq!(
        gpu.target_size(),
        wgpu::Extent3d {
            width: resized.width.div_ceil(scale),
            height: resized.height.div_ceil(scale),
            ..size
        }
    );
    assert!(
        !trace.headless,
        "live preparation must not use the fixture shortcut"
    );
    trace.set(key, None);
    assert!(
        !trace.ready(),
        "removing geometry invalidates transport immediately"
    );
    while trace.active_revision != trace.worker.revision {
        trace.install(&device);
        assert!(
            Instant::now() < deadline,
            "removal did not reach scene admission"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(!trace.ready());
    assert!(
        trace.gpu.is_none(),
        "empty scenes must return to the safe fallback"
    );
}
