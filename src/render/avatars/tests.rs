use super::*;
use crate::content::Catalog;

/// Runs without a window; exercises the actual avatar mesh, pipeline, palette
/// shader and instance bytes. Set BLOXGLOOM_APPEARANCE_PREVIEW to retain a PNG.
#[test]
fn gpu_registered_player_palettes_preserve_default_and_color_all_three_parts() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let builtin = Catalog::builtins();
    let mut authored = builtin.clone();
    authored
        .register_player_appearance(bloxgloom_host_api::appearance::Appearance {
            key: "demo:wardrobe".into(),
            revision: 1,
            model: bloxgloom_host_api::appearance::MODEL.into(),
            palettes: [
                vec![[0.0, 1.0, 0.0]],
                vec![[1.0, 0.0, 0.0]],
                vec![[0.0, 0.0, 1.0]],
            ],
        })
        .unwrap();
    let original = render(&device, &queue, &builtin, [0; 4]);
    let changed = render(&device, &queue, &authored, [6, 8, 6, 0]);
    for row in 0..HEIGHT as usize {
        let start = row * WIDTH as usize * 4;
        assert_eq!(
            &original[start..start + WIDTH as usize * 2],
            &changed[start..start + WIDTH as usize * 2],
            "builtin default changed"
        );
    }
    // Samples are on the front face, avoiding eyes, seams, hair and silhouettes.
    for (x, y, channel) in [(0.65, 1.43, 1), (0.65, 1.0, 0), (0.53, 0.35, 2)] {
        let px = ((x + 1.4) / 2.8 * WIDTH as f32) as usize;
        let py = ((1.9 - y) / 2.0 * HEIGHT as f32) as usize;
        let pixel = &changed[(py * WIDTH as usize + px) * 4..][..4];
        assert!(pixel[channel] > 25, "part not drawn: {pixel:?}");
        for other in 0..3 {
            if other != channel {
                assert!(pixel[other] < 3, "wrong palette part: {pixel:?}");
            }
        }
    }
    if let Some(path) = std::env::var_os("BLOXGLOOM_APPEARANCE_PREVIEW") {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&changed)
            .unwrap();
    }
}

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    catalog: &Catalog,
    selection: [u8; 4],
) -> Vec<u8> {
    let camera = glam::camera::rh::proj::directx::orthographic(-1.4, 1.4, -0.1, 1.9, 0.1, 10.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&camera.to_cols_array()),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut renderer =
        AvatarRenderer::new(device, wgpu::TextureFormat::Rgba8Unorm, &camera, catalog);
    let avatars = [(-0.65, [0; 4]), (0.65, selection)].map(|(x, cosmetics)| VisualAvatar {
        animation: Default::default(),
        model: AvatarModel::Player,
        pose: [0.0; 4],
        airborne: false,
        id: 1,
        position: Vec3::new(x, 0.0, 0.0),
        cosmetics,
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
    });
    renderer.set(queue, &avatars);
    let size = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color.create_view(&Default::default()),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.create_view(&Default::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        assert!(renderer.draw(&mut pass) > 0);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
        },
        size,
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
    let bytes = readback.slice(..).get_mapped_range().unwrap().to_vec();
    readback.unmap();
    bytes
}
