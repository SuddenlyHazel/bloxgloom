use super::*;

const SHADER: &str =
    include_str!("../../../fixtures/effect-packages/sepia/assets/shaders/sepia.wgsl");

#[test]
fn composed_resources_require_exact_direct_dependencies() {
    use crate::server::client_bundle::ClientPackage;
    let package = |output: &str, input: &str, final_output: bool, dependencies| {
        ClientPackage {
        version: "1.0.0".into(), dependencies, sources: BTreeMap::new(), textures: BTreeMap::new(),
        model_assets: BTreeMap::new(), ui_assets: BTreeMap::new(), material_assets: BTreeMap::new(), sound_assets: BTreeMap::new(),
        effect_assets: BTreeMap::from([
            ("pass".into(),(7,format!(r#"{{"version":2,"shader":"shader","inputs":["{input}"],"output":"{output}","final":{final_output}}}"#).into_bytes())),
            ("shader".into(),(6,b"fn effect_fragment(uv:vec2f)->vec4f { return effect_input(uv,0u); }".to_vec())),
        ]),
    }
    };
    let mut packages = BTreeMap::from([
        (
            "base".into(),
            package("base:color", SCENE, false, BTreeMap::new()),
        ),
        (
            "shade".into(),
            package(
                "shade:color",
                "base:color",
                true,
                BTreeMap::from([("base".into(), "1.0.0".into())]),
            ),
        ),
    ]);
    let prepared = prepare(&packages).unwrap().unwrap();
    assert_eq!(prepared.passes[0].owner, "base:pass");
    assert_eq!(prepared.passes[1].owner, "shade:pass");
    packages.get_mut("shade").unwrap().dependencies.clear();
    assert!(
        prepare(&packages)
            .unwrap_err()
            .contains("exact direct dependency")
    );
}

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
fn gpu_composition_error_is_returned_with_package_resource_context() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/visual-packages");
    let snapshot = crate::server::PackageSnapshot::discover(&root).unwrap();
    let mut prepared = snapshot.client_bundle().effect().unwrap().as_ref().clone();
    // A pure helper can pass the authored-hook validator and then collide with
    // a renderer entrypoint in the composed module. Backend failures must be
    // returned by preparation rather than escaping an uncaptured GPU error.
    prepared.passes[0]
        .source
        .push_str("\nfn fs_main() -> f32 { return 1.0; }");
    shader::validate(&prepared.passes[0].source).unwrap();
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, _) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let result = Effect::prepare(&device, &prepared);
    let Err(error) = result else {
        panic!("invalid composition installed")
    };
    assert!(
        error.contains("prism:grade") && error.contains("shader pipeline"),
        "{error}"
    );
}

#[test]
fn verified_example_gpu_pass_survives_resize_and_grades_scene() {
    gpu_preview(false);
}

#[test]
fn version_two_graph_composes_declared_inputs_and_parameters_on_gpu() {
    gpu_preview(true);
}

fn gpu_preview(extended: bool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if extended {
        "fixtures/visual-packages"
    } else {
        "fixtures/effect-packages"
    });
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
    if extended {
        post.set_parameter(&crate::render::parameters::Update {
            resource: "prism:mix".into(),
            name: "strength".into(),
            value: crate::render::parameters::Value::Scalar(1.0),
        })
        .unwrap();
    }
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
