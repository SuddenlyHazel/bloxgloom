use super::*;

const SHADER: &str =
    include_str!("../../../fixtures/effect-packages/sepia/assets/shaders/sepia.wgsl");

#[test]
fn fragment_contract_rejects_unbounded_work_and_foreign_bindings() {
    validate(SHADER).unwrap();
    for shader in [
        SHADER.replace("@binding(0)", "@binding(3)"),
        SHADER.replace("@binding(1)", "@binding(0)"),
        SHADER.replace("fs_main", "other"),
        SHADER.replace("let uv", "loop {} let uv"),
        SHADER.replace("@group(0)", "@group(1)"),
        SHADER.replace("var<uniform>", "var<storage, read>"),
        SHADER.replace("let uv", "var huge: array<vec4f, 1000000>; let uv"),
        SHADER.replace("texture_2d<f32>", "texture_storage_2d<rgba16float, write>"),
        format!("{SHADER}\nfn helper() {{}}"),
        format!("{SHADER}\n@compute @workgroup_size(1) fn work() {{}}"),
    ] {
        assert!(validate(&shader).is_err(), "{shader}");
    }
}

#[test]
fn verified_example_gpu_pass_survives_resize_and_grades_scene() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/effect-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    let prepared = snapshot.client_bundle().effect().unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut post = crate::render::post::PostProcess::new(&device, 7, 5, format);
    post.install_effect(&device, prepared).unwrap();
    post.resize(&device, 1, 1);
    post.resize(&device, 7, 5);
    post.configure(&queue, false, 1.0, 0.0);
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("effect behavioral output"),
        size: wgpu::Extent3d {
            width: 7,
            height: 5,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &post.scene,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.5,
                        b: 0.5,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    post.encode(
        &device,
        &queue,
        &mut encoder,
        &output.create_view(&Default::default()),
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    assert!(
        bytes[0] > bytes[1] + 10 && bytes[1] > bytes[2] + 10,
        "package sepia must change neutral scene pixels: {:?}",
        &bytes[..4]
    );
    assert_eq!(bytes[3], 255);
}
