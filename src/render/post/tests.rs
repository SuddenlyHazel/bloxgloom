use super::*;

// Independently reduce composite5.glsl's checked-in defaults to neutral gray:
// the desaturation matrix and its inverse preserve gray, EXPOSURE=0 gives
// exp2(2)=4, WHITE_CURVE=2 and both contrast curves are 1. The first copied
// pixel also exercises its default vignette before display transfer.
fn displayed_gray(level: f64, width: u32, height: u32) -> u8 {
    let mapped = if crate::render::sky::style_enabled() {
        let distance = (0.5 / f64::from(width) - 0.5).hypot(0.5 / f64::from(height) - 0.5);
        let vignette = 1.0 - (distance * distance * 0.3535 + distance * 0.75) * 1.06;
        let exposed = level * vignette * 4.0;
        exposed / (exposed * exposed + 1.0).sqrt()
    } else {
        level / (1.0 + level * level).sqrt()
    };
    let encoded = if mapped <= 0.0031308 {
        mapped * 12.92
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

#[test]
fn gpu_post_preserves_black_hdr_highlights_and_output_transfer() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        // Odd dimensions exercise the reduced-resolution allocation; 1x1 is also valid.
        for (width, height) in [(1, 1), (7, 5)] {
            let mut transfer_results = Vec::new();
            for format in [
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureFormat::Rgba8Unorm,
            ] {
                let mut post = PostProcess::new(&device, width, height, format);
                post.resize(&device, height, width);
                post.resize(&device, width, height);
                let output = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("post test output"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = output.create_view(&Default::default());
                let mut pixels = Vec::new();
                // Exercise bloom on -> off with the same resources, so stale bloom cannot survive.
                for (level, bloom, enabled, exposure) in [
                    (0.0, 0.12, true, 1.0),
                    (0.5, 0.0, true, 1.0),
                    (2.0, 0.12, true, 1.0),
                    (2.0, 0.0, true, 1.0),
                    (4.0, 0.0, true, 1.0),
                    (0.0, 0.0, true, 1.0),
                    (0.5, 1.0, false, 4.0),
                    (0.0, 1.0, false, 4.0),
                    (0.5, 0.0, true, 1.0),
                    (0.004, 0.0, true, 1.0),
                    (0.02, 0.0, true, 1.0),
                    (0.08, 0.0, true, 1.0),
                ] {
                    post.configure(&queue, enabled, exposure, bloom);
                    let readback = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("post test readback"),
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
                                        r: level,
                                        g: level,
                                        b: level,
                                        a: 1.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                    }
                    post.encode(&device, &queue, &mut encoder, &view);
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
                    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                    rx.recv().unwrap().unwrap();
                    let bytes = readback.slice(..).get_mapped_range().unwrap();
                    pixels.push(bytes[0]);
                    assert_eq!(bytes[3], 255);
                }
                assert_eq!(pixels[0], 0, "bloom must not add a brightness floor");
                assert_eq!(pixels[5], 0, "disabled bloom must not retain old light");
                assert!(
                    (187..=189).contains(&pixels[6]),
                    "master off must bypass exposure and tone mapping"
                );
                assert_eq!(pixels[7], 0, "master off must bypass stale bloom");
                assert_eq!(
                    pixels[8], pixels[1],
                    "re-enabling must restore tone mapping"
                );
                for (index, level) in [
                    (1, 0.5),
                    (3, 2.0),
                    (4, 4.0),
                    (9, 0.004),
                    (10, 0.02),
                    (11, 0.08),
                ] {
                    let expected = displayed_gray(level, width, height);
                    assert!(
                        pixels[index].abs_diff(expected) <= 2,
                        "tone/vignette/display oracle at {level}: {} vs {expected}",
                        pixels[index]
                    );
                }
                assert!(pixels[2] >= pixels[3], "bloom must add light");
                assert!(
                    pixels[1] < pixels[3] && pixels[3] < pixels[4],
                    "HDR values must retain highlight gradation"
                );
                assert!(
                    pixels[9] >= 12 && pixels[10] >= 37,
                    "dark material detail was crushed by the tone-map toe: {pixels:?}"
                );
                assert!(
                    pixels[9] < pixels[10] && pixels[10] < pixels[11],
                    "shaded radiance must remain distinguishable"
                );
                transfer_results.push(pixels);
            }
            for (hardware, explicit) in transfer_results[0].iter().zip(&transfer_results[1]) {
                assert!(
                    hardware.abs_diff(*explicit) <= 1,
                    "sRGB and linear attachments must display the same result"
                );
            }
        }
    });
}
