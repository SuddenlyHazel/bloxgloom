use super::*;

// Fixture uses only finite positive normal binary16 values.
fn half_bits(value: f64) -> u16 {
    assert!((0.00006103515625..65504.0).contains(&value));
    let bits = (value as f32).to_bits();
    let rounded = bits + 0xfff + ((bits >> 13) & 1);
    ((rounded >> 13) - (112 << 10)) as u16
}
fn half_value(bits: u16) -> f64 {
    if bits == 0 {
        return 0.0;
    }
    (1.0 + f64::from(bits & 1023) / 1024.0) * 2.0_f64.powi(i32::from(bits >> 10) - 15)
}

// Independent f64 reduction of composite4/5.glsl, retaining GL coordinates.
// These constants deliberately do not read the WGSL implementation.
const TAP: [f64; 6] = [0.03, 0.15, 0.32, 0.32, 0.15, 0.03];
const RADIUS: [f64; 7] = [4.0, 3.18, 2.52, 2.0, 1.59, 1.26, 1.0];
fn offset(level: usize, w: usize, h: usize) -> [f64; 2] {
    let base = [
        [0.0, 0.0, 0.0, 0.0],
        [0.5, 0.0, 4.0, 0.0],
        [0.5, 0.25, 4.0, 4.0],
        [0.625, 0.25, 8.0, 4.0],
        [0.6875, 0.25, 12.0, 4.0],
        [0.625, 0.3125, 8.0, 8.0],
        [0.640625, 0.3125, 12.0, 8.0],
    ][level - 1];
    [base[0] + base[2] / w as f64, base[1] + base[3] / h as f64]
}
fn bayer2(x: f64, y: f64) -> f64 {
    (x.floor() * 0.5 + y.floor().powi(2) * 0.75).fract()
}
fn bayer8(x: f64, y: f64) -> f64 {
    bayer2(x * 0.25, y * 0.25) / 16.0 + bayer2(x * 0.5, y * 0.5) / 4.0 + bayer2(x, y)
}
fn decode(value: f64) -> f64 {
    value.powi(4) * 32.0
}

#[test]
fn independent_default_kernel_encoding_goldens() {
    assert!((TAP.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((RADIUS.iter().sum::<f64>() - 15.55).abs() < 1e-12);
    let bayer: Vec<_> = (0..8)
        .flat_map(|y| (0..8).map(move |x| bayer8(x as f64 + 0.5, y as f64 + 0.5)))
        .collect();
    let mut sorted = bayer.clone();
    sorted.sort_by(f64::total_cmp);
    for (index, value) in sorted.iter().enumerate() {
        assert_eq!(*value, index as f64 / 64.0);
    }
    assert_eq!(bayer8(0.5, 0.5), 0.0);
    assert_eq!(bayer8(1.5, 0.5), 0.5);
    assert_eq!(bayer8(0.5, 1.5), 0.75);
    for (level, encoded, restored) in [
        (0.02, 40u8, 0.019374442828823564),
        (0.5, 90, 0.496545778905904),
        (8.0, 180, 7.944732462494464),
    ] {
        let byte = ((level / 32.0_f64).powf(0.25) * 255.0).round() as u8;
        assert_eq!(byte, encoded);
        assert!((decode(f64::from(byte) / 255.0) - restored).abs() < 1e-12);
    }
}

struct Image {
    w: usize,
    h: usize,
    pixels: Vec<[f64; 3]>,
}
impl Image {
    fn sample(&self, uv: [f64; 2]) -> [f64; 3] {
        let p = [uv[0] * self.w as f64 - 0.5, uv[1] * self.h as f64 - 0.5];
        let base = [p[0].floor(), p[1].floor()];
        let f = [p[0] - base[0], p[1] - base[1]];
        let mut value = [0.0; 3];
        for dy in 0..2 {
            for dx in 0..2 {
                let x = (base[0] as isize + dx).clamp(0, self.w as isize - 1) as usize;
                let y = (base[1] as isize + dy).clamp(0, self.h as isize - 1) as usize;
                let weight = if dx == 0 { 1.0 - f[0] } else { f[0] }
                    * if dy == 0 { 1.0 - f[1] } else { f[1] };
                for (channel, output) in value.iter_mut().enumerate() {
                    *output += self.pixels[y * self.w + x][channel] * weight;
                }
            }
        }
        value
    }
    fn downsample(&self) -> Self {
        let w = (self.w / 2).max(1);
        let h = (self.h / 2).max(1);
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let mut color = [0.0; 3];
                for dy in [-0.5, 0.5] {
                    for dx in [-0.5, 0.5] {
                        let sample = self.sample([
                            (x as f64 + 0.5) / w as f64 + dx / self.w as f64,
                            (y as f64 + 0.5) / h as f64 + dy / self.h as f64,
                        ]);
                        for c in 0..3 {
                            color[c] += sample[c] * 0.25;
                        }
                    }
                }
                pixels.push(color.map(|v| half_value(half_bits(v))));
            }
        }
        Self { w, h, pixels }
    }
}
fn mip_sample(mips: &[Image], uv: [f64; 2], lod: f64) -> [f64; 3] {
    let lod = lod.clamp(0.0, (mips.len() - 1) as f64);
    let low = lod.floor() as usize;
    let high = (low + 1).min(mips.len() - 1);
    let a = mips[low].sample(uv);
    let b = mips[high].sample(uv);
    std::array::from_fn(|c| a[c] * (1.0 - lod.fract()) + b[c] * lod.fract())
}
fn atlas_pixel(mips: &[Image], x: usize, y: usize) -> [u8; 3] {
    let w = mips[0].w;
    let h = mips[0].h;
    let k = h as f64 * 0.8 / (h as f64).min(720.0);
    let gl = [
        (x as f64 + 0.5) / w as f64,
        1.0 - (y as f64 + 0.5) / h as f64,
    ];
    let mut result = [0.0; 3];
    for level in 1..=7 {
        let scale = 2.0_f64.powi(level as i32);
        let off = offset(level, w, h);
        let coord = [(gl[0] * k - off[0]) * scale, (gl[1] * k - off[1]) * scale];
        if (coord[0] - 0.5).abs() >= 0.5 + 2.0 * scale / w as f64
            || (coord[1] - 0.5).abs() >= 0.5 + 2.0 * scale / h as f64
        {
            continue;
        }
        for (i, wi) in TAP.iter().enumerate() {
            for (j, wj) in TAP.iter().enumerate() {
                let uv = [
                    coord[0] + (i as f64 - 2.5) * k * scale / w as f64,
                    1.0 - coord[1] - (j as f64 - 2.5) * k * scale / h as f64,
                ];
                let value = mip_sample(mips, uv, level as f64 + k.log2());
                for c in 0..3 {
                    result[c] += value[c] * wi * wj;
                }
            }
        }
    }
    let noise = (bayer8(x as f64 + 0.5, h as f64 - y as f64 - 0.5) - 0.5) / 384.0;
    result.map(|value| (((value / 32.0).powf(0.25) + noise).clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn shader_validation(source: &str) {
    let module = wgpu::naga::front::wgsl::parse_str(source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
#[test]
fn reference_bloom_production_shaders_validate() {
    shader_validation(&format!("{COMMON}\n{}", include_str!("pack.wgsl")));
    shader_validation(&format!(
        "{}\nconst BG_BSL_STYLE: bool = true;\nconst BG_REFERENCE_BLOOM: bool = true;\nconst BG_REFERENCE_DISPLAY: bool = false;\n{COMMON}\n{}\n{COMPOSITE}",
        crate::render::sky::STYLE_SHADER,
        include_str!("../../post.wgsl")
    ));
}

#[test]
fn gpu_actual_reference_atlas_matches_independent_source_kernel() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let (w, h) = (321usize, 181usize);
        let mut post = super::super::PostProcess::new_with_reference(
            &device,
            w as u32,
            h as u32,
            wgpu::TextureFormat::Rgba8Unorm,
            true,
        );
        post.resize(&device, 7, 5);
        post.resize(&device, w as u32, h as u32);
        let pixels: Vec<_> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                // Dim baseline, asymmetric gradient and an HDR emitter reveal
                // orientation, all-scene blur, tile leakage and highlight spread.
                let burst = if (x as isize - 76).abs() < 12 && (y as isize - 131).abs() < 12 {
                    5.0
                } else {
                    0.0
                };
                [
                    0.02 + x as f64 / w as f64 * 0.2 + burst,
                    0.04 + y as f64 / h as f64 * 0.3,
                    0.08,
                ]
                .map(|value| half_value(half_bits(value)))
            })
            .collect();
        let data: Vec<_> = pixels
            .iter()
            .flat_map(|color| {
                [
                    half_bits(color[0]),
                    half_bits(color[1]),
                    half_bits(color[2]),
                    0x3c00u16,
                ]
            })
            .collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: post.scene.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w as u32 * 8),
                rows_per_image: Some(h as u32),
            },
            post.scene.texture().size(),
        );
        let mut mips = vec![Image { w, h, pixels }];
        while mips.last().unwrap().w > 1 || mips.last().unwrap().h > 1 {
            mips.push(mips.last().unwrap().downsample());
        }
        let reference = post.reference_bloom.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        reference.encode(&mut encoder);
        let row = (w as u32 * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row) * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: reference.atlas.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h as u32),
                },
            },
            reference.atlas.texture().size(),
        );
        queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let bytes = readback.slice(..).get_mapped_range().unwrap();
        // Tile centers and gutters, top/bottom boundaries, emitter and dim regions.
        for (x, y) in [
            (0, 0),
            (10, 160),
            (70, 120),
            (100, 120),
            (210, 155),
            (221, 149),
            (252, 114),
            (280, 110),
            (310, 110),
            (259, 97),
            (264, 98),
            (320, 180),
            (180, 180),
        ] {
            let expected = atlas_pixel(&mips, x, y);
            let actual = &bytes[y * row as usize + x * 4..][..3];
            for c in 0..3 {
                assert!(
                    actual[c].abs_diff(expected[c]) <= 1,
                    "source atlas ({x},{y}) channel {c}: {actual:?} vs {expected:?}"
                );
            }
        }
        // Reconstruct using the actual production composite helper, with only
        // display mapping replaced by identity for an independent HDR oracle.
        drop(bytes);
        readback.unmap();
        gpu_reconstruction(&device, &queue, &post, &mips);
    });
}

fn gpu_reconstruction(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    post: &super::super::PostProcess,
    mips: &[Image],
) {
    let (w, h) = (mips[0].w, mips[0].h);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("production bloom reconstruction fixture"), source: wgpu::ShaderSource::Wgsl(format!("{COMMON}\n{}\n{COMPOSITE}\n@fragment fn reconstructed(input: Vertex) -> @location(0) vec4f {{ return vec4f(bg_reference_bloom(textureSampleLevel(scene, linear_sampler, input.uv, 0.0).rgb, input.uv), 1.0); }}", include_str!("pack.wgsl").replace("unused_glow", "bloom")).into()) });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&post.layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("reconstructed"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: post.scene.texture().size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    draw(
        &mut encoder,
        &pipeline,
        &post.reference_bloom.as_ref().unwrap().composite_group,
        &output.create_view(&Default::default()),
    );
    let row = (w as u32 * 8).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row) * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
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
                bytes_per_row: Some(row),
                rows_per_image: Some(h as u32),
            },
        },
        output.size(),
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    for (x, y) in [
        (13, 15),
        (74, 129),
        (98, 129),
        (115, 129),
        (255, 45),
        (300, 170),
    ] {
        let mut expected = mips[0].pixels[y * w + x].map(|v| v * 0.8);
        for level in 1..=7 {
            let off = offset(level, w, h);
            let scale = 2.0_f64.powi(level as i32);
            let res = 1.25 * (h as f64).min(720.0) / h as f64;
            let uv = [
                ((x as f64 + 0.5) / w as f64 / scale + off[0] + 0.5 / w as f64) * res,
                1.0 - ((1.0 - (y as f64 + 0.5) / h as f64) / scale + off[1]) * res,
            ];
            let p = [uv[0] * w as f64 - 0.5, uv[1] * h as f64 - 0.5];
            let base = [p[0].floor(), p[1].floor()];
            let f = [p[0] - base[0], p[1] - base[1]];
            let mut encoded = [0.0; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let ax = (base[0] as isize + dx).clamp(0, w as isize - 1) as usize;
                    let ay = (base[1] as isize + dy).clamp(0, h as isize - 1) as usize;
                    let color = atlas_pixel(mips, ax, ay);
                    let weight = if dx == 0 { 1.0 - f[0] } else { f[0] }
                        * if dy == 0 { 1.0 - f[1] } else { f[1] };
                    for c in 0..3 {
                        encoded[c] += f64::from(color[c]) / 255.0 * weight;
                    }
                }
            }
            for c in 0..3 {
                expected[c] += decode(encoded[c]) * RADIUS[level - 1] / 15.55 * 0.2;
            }
        }
        for (c, expected) in expected.iter().enumerate() {
            let start = y * row as usize + x * 8 + c * 2;
            let actual = half_value(u16::from_le_bytes([bytes[start], bytes[start + 1]]));
            assert!(
                (actual - expected).abs() < 0.012 * expected.max(0.05),
                "source reconstruction ({x},{y}) c{c}: {actual} vs {expected}",
            );
        }
    }
}

#[test]
fn gpu_reference_post_strength_disable_and_black_contract() {
    pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let mut post = super::super::PostProcess::new_with_reference(
            &device,
            96,
            64,
            wgpu::TextureFormat::Rgba8Unorm,
            true,
        );
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: post.scene.texture().size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut observed = Vec::new();
        for (level, strength, effects) in [
            (0.0, 1.0, true),
            (0.08, 0.01, true),
            (0.08, 1.0, true),
            (0.08, 5.0, true),
            (0.08, 0.0, true),
            (0.5, 1.0, false),
            (0.0, 1.0, false),
            (0.0, 0.0, true),
        ] {
            post.configure(&queue, effects, 1.0, strength);
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
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
            post.encode(&device, &queue, &mut encoder, &output_view);
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &output,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 48, y: 32, z: 0 },
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
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            {
                let bytes = readback.slice(..).get_mapped_range().unwrap();
                observed.push(bytes[0]);
                assert_eq!(bytes[3], 255);
            }
            readback.unmap();
        }
        assert_eq!(observed[0], 0, "source bloom cannot add a black floor");
        assert_eq!(
            observed[1], observed[2],
            "positive enhanced slider does not alter source strength 1"
        );
        assert_eq!(observed[2], observed[3]);
        assert_eq!(
            observed[5], 188,
            "master off bypasses source bloom/tone/vignette"
        );
        assert_eq!(observed[6], 0);
        assert_eq!(observed[7], 0, "off must not retain old atlas");
        assert!(observed[4] > 0, "explicit bloom off retains scene radiance");
    });
}
