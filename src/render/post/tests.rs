use super::*;

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
                assert!(
                    (175..=185).contains(&pixels[1]),
                    "mid-gray must be encoded to sRGB exactly once"
                );
                assert!(pixels[2] >= pixels[3], "bloom must add light");
                assert!(
                    pixels[4] > pixels[3] && pixels[4] < 255,
                    "HDR values must retain highlight gradation"
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
