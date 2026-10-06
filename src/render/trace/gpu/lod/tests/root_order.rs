//! A later page is visited first, but a lower-ID exact tie must still win.
use super::*;

pub(super) fn check(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    coverage: &wgpu::Buffer,
    output: &wgpu::Buffer,
    run: impl Fn(&wgpu::BindGroup) -> Vec<u32>,
) {
    let page = || {
        Scene::build([Arc::new(Chunk {
            triangles: vec![plane(2.0, 0)],
            ..Default::default()
        })])
    };
    let mut scenes = vec![page(), Scene::build([]), Scene::build([]), page()];
    // Reversed root entry distances force page3 before page0; both actual
    // triangles remain at2m. Tightening must not exclude the equal root.
    scenes[3].nodes[0].min[0] = 1.0;
    let lod = LodGeometry::new(device, &scenes);
    let mut entries = lod.entries().to_vec();
    entries.extend([
        wgpu::BindGroupEntry {
            binding: 10,
            resource: coverage.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 18,
            resource: output.as_entire_binding(),
        },
    ]);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("root-distance reversed exact triangle ties"),
        layout,
        entries: &entries,
    });
    let words = run(&group);
    assert_eq!(f32::from_bits(words[0]), 2.0);
    assert_eq!(
        words[1], 0x10000000,
        "smaller encoded ID must win after reverse root visit"
    );
    assert_eq!(words[11], 7, "native-near exact tie remains preferred");
    assert_eq!(
        words[12], 0,
        "ordered page hits/UV/normals differ from original fixed-order full decoder"
    );
}
