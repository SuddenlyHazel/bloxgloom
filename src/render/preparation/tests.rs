use super::*;

#[test]
fn asynchronous_gpu_candidate_contains_both_materials_and_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/visual-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    let bundle = snapshot.client_bundle();
    let catalog = std::sync::Arc::new(
        crate::server::package_catalog_for_preview(crate::content::Catalog::builtins(), &root)
            .unwrap(),
    );
    let material = bundle.material().unwrap().resolve(&catalog).unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        apply_limit_buckets: false,
        ..Default::default()
    }))
    .unwrap();
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: crate::render::material_device_limits(
                adapter.limits(),
                crate::render::material_texture_layers(&catalog) as usize,
            )
            .unwrap(),
            ..Default::default()
        }),
    )
    .unwrap();
    let mut preparation = Preparation::start(
        device.clone(),
        queue.clone(),
        catalog.clone(),
        Some(&material),
        bundle.effect().map(|e| e.as_ref()),
    )
    .unwrap();
    // Full material arrays are deliberately prepared off-thread. This test
    // verifies asynchronous composition, not GPU upload latency under parallel
    // headless tests; retain a generous watchdog for a stuck worker.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let ready = loop {
        if let Some(result) = preparation.poll() {
            break result.unwrap();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(ready.material.is_some() && ready.effect.is_some());
    preparation.finish();
    // Abandoning a candidate retires its result, with no global registration
    // for a later attempt to inherit. The next attempt may have no visuals.
    drop(ready);
    let mut empty = Preparation::start(device, queue, catalog, None, None).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let ready = loop {
        if let Some(result) = empty.poll() {
            break result.unwrap();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(ready.material.is_none() && ready.effect.is_none());
    empty.finish();
}
