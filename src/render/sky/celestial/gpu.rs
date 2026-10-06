use crate::render::{Camera, SkyRenderer, daylight::Atmosphere, sky};
use glam::Vec3;

const SIZE: u32 = 513;

fn camera(sun: Vec3, position: Vec3) -> Camera {
    Camera {
        position,
        yaw: sun.z.atan2(sun.x),
        pitch: sun.y.asin(),
        fov_y_radians: 20.0f32.to_radians(),
    }
}

fn projected_sun(sun: Vec3) -> [f64; 2] {
    let camera = camera(sun, Vec3::ZERO);
    // The actual camera clamps pitch to1.55: a zenith Sun is visible slightly
    // above screen center. Locate its direction, not the requested camera aim.
    let forward = camera.direction();
    let right = Vec3::new(-camera.yaw.sin(), 0.0, camera.yaw.cos());
    let up = right.cross(forward).normalize();
    let scale = 2.0 * (camera.fov_y_radians * 0.5).tan() * sun.dot(forward);
    [
        f64::from(SIZE) * (0.5 + f64::from(sun.dot(right) / scale)),
        f64::from(SIZE) * (0.5 - f64::from(sun.dot(up) / scale)),
    ]
}

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sky: SkyRenderer,
    legacy: wgpu::RenderPipeline,
    source_celestial: wgpu::RenderPipeline,
    oracle: bool,
    target: Option<Vec3>,
    phase: u32,
    targets: Vec<wgpu::Texture>,
    depth: wgpu::TextureView,
    read: wgpu::Buffer,
}

impl Fixture {
    fn new() -> Self {
        let adapter =
            pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default()))
                .expect("celestial alignment GPU adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let format = crate::render::post::HDR_FORMAT;
        let sky = SkyRenderer::new(&device, 1, 1, format);
        let source = sky::shader_source();
        let current = "let sun_uv=bg_sky_sun_uv(ray,sun_direction);";
        assert_eq!(source.matches(current).count(), 1);
        // Independent source baseline uses the exact old centered projection;
        // it does not reuse the new enhanced/reference branch's decision.
        let legacy = sky::pipeline::create_with_source(
            &device,
            format,
            &sky.layout,
            source.replace(
                current,
                "let sun_uv=bg_sky_celestial_uv(ray,sun_direction,0.30);",
            ),
        );
        let source_celestial = sky::pipeline::create_with_source(
            &device,
            format,
            &sky.layout,
            format!(
                "{}\n{}",
                source
                    .replace(
                        "bg_bsl_textured_celestial(sun_texel",
                        "source_celestial(sun_texel"
                    )
                    .replace(
                        "bg_bsl_textured_celestial(moon_texel",
                        "source_celestial(moon_texel"
                    )
                    .replace(
                        "bg_bsl_reference_star_fade(sky_camera.eye.y",
                        "source_star_fade(sky_camera.eye.y"
                    ),
                include_str!("../reference_celestial/oracle.wgsl")
            ),
        );
        let targets = (0..4)
            .map(|_| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("production celestial HDR output"),
                    size: wgpu::Extent3d {
                        width: SIZE,
                        height: SIZE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            })
            .collect();
        let depth = crate::render::visibility::create_depth(&device, SIZE, SIZE);
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("celestial HDR core readback"),
            size: u64::from(Self::stride() * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            device,
            queue,
            sky,
            legacy,
            source_celestial,
            oracle: false,
            target: None,
            phase: 0,
            targets,
            depth,
            read,
        }
    }

    fn stride() -> u32 {
        (SIZE * 8).div_ceil(256) * 256
    }

    fn draw(&mut self, sun: Vec3, reference: f32, legacy: bool, position: Vec3) -> Vec<[f32; 3]> {
        let camera = camera(self.target.unwrap_or(sun), position);
        let mut atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
        atmosphere.scene_transport = true;
        self.sky.configure(atmosphere);
        let mut data = sky::sky_camera_data(camera, SIZE, SIZE, atmosphere);
        data[3] = 0.0;
        data[12..15].copy_from_slice(&sun.to_array());
        data[15] = 1.0; // Clear current camera medium; skip raster clouds.
        data[16..24].fill(0.0);
        data[24..28].copy_from_slice(&[1.0; 4]);
        data[31] = 0.0; // Zero base sky isolates actual celestial artwork.
        data[32] = 0.0;
        data[33] = [1.0, 0.875, 0.75, 0.625, 0.5, 0.625, 0.75, 0.875][self.phase as usize];
        data[34] = self.phase as f32;
        data[39] = reference;
        self.queue
            .write_buffer(&self.sky.camera, 0, bytemuck::cast_slice(&data));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.sky.prepare(&self.device, &mut encoder, SIZE, SIZE);
        self.sky.group = SkyRenderer::bind(
            &self.device,
            &self.sky.layout,
            &self.sky.camera,
            &self.sky.clouds.view,
            &self.sky.sampler,
            &self.sky.celestial,
            reference.abs() > 0.5,
        );
        let views: Vec<_> = self
            .targets
            .iter()
            .map(|texture| texture.create_view(&Default::default()))
            .collect();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("actual enhanced/reference Sun core projection"),
                color_attachments: &crate::render::scene_ao::attachments(
                    &views[0],
                    &views[1],
                    &views[2],
                    &views[3],
                    wgpu::Color::BLACK,
                ),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(if self.oracle {
                &self.source_celestial
            } else if legacy {
                &self.legacy
            } else {
                &self.sky.pipeline
            });
            pass.set_bind_group(0, &self.sky.group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            self.targets[0].as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(Self::stride()),
                    rows_per_image: Some(SIZE),
                },
            },
            self.targets[0].size(),
        );
        self.queue.submit([encoder.finish()]);
        self.read
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let mapped = self.read.slice(..).get_mapped_range().unwrap();
        let mut pixels = Vec::with_capacity((SIZE * SIZE) as usize);
        for row in mapped.chunks_exact(Self::stride() as usize) {
            for pixel in row[..(SIZE * 8) as usize].chunks_exact(8) {
                pixels.push(std::array::from_fn(|i| {
                    half(u16::from_le_bytes([pixel[i * 2], pixel[i * 2 + 1]]))
                }));
            }
        }
        drop(mapped);
        self.read.unmap();
        pixels
    }
}

#[test]
fn gpu_production_textured_celestial_matches_source_default_at_horizon_and_phases() {
    let mut fixture = Fixture::new();
    for moon in [false, true] {
        for elevation in [-0.04f32, 0.0, 0.04, 0.3, 0.8] {
            let direction = Vec3::new((1.0 - elevation * elevation).sqrt(), elevation, 0.0);
            let sun = if moon { -direction } else { direction };
            // Aim at Moon as well as Sun; a negative solar direction chooses
            // the real atlas and exercises the night palette/desaturation.
            let aim = if moon { -sun } else { sun };
            // draw() ordinarily aims at the Sun; override the camera basis by
            // temporarily swapping only the camera target in the fixture.
            fixture.target = Some(aim);
            fixture.phase = if elevation < 0.0 {
                4
            } else if elevation < 0.1 {
                3
            } else {
                0
            };
            for reference in [-1.0, 1.0] {
                fixture.oracle = false;
                let current =
                    fixture.draw(sun, reference, false, Vec3::new(-32000.0, 70.0, 32000.0));
                fixture.oracle = true;
                let source =
                    fixture.draw(sun, reference, false, Vec3::new(-32000.0, 70.0, 32000.0));
                let mut maximum = 0.0f32;
                for (a, b) in current.iter().zip(&source) {
                    for i in 0..3 {
                        assert!(
                            (a[i] - b[i]).abs() <= 0.003 + 0.002 * b[i].abs(),
                            "moon{moon} elevation{elevation} reference{reference}: current{a:?} source{b:?}"
                        );
                        maximum = maximum.max(a[i]);
                    }
                }
                if elevation >= 0.3 {
                    assert!(maximum > 0.005, "actual art must reach fragment output");
                }
            }
        }
    }
}

#[test]
fn gpu_production_reference_stars_follow_bedrock_while_enhanced_is_unchanged() {
    let mut fixture = Fixture::new();
    fixture.target = Some(Vec3::new(0.0, 0.7, 0.7).normalize());
    let sun = Vec3::new(1.0, -0.8, 0.0).normalize();
    let enhanced = fixture.draw(sun, 0.0, false, Vec3::new(0.0, 70.0, 0.0));
    let below = fixture.draw(sun, 0.0, false, Vec3::new(0.0, -100.0, 0.0));
    assert_eq!(
        enhanced, below,
        "enhanced actual star pixels must be unchanged by reference-only bedrock visibility"
    );
    for reference in [-1.0, 1.0] {
        let upper = fixture.draw(sun, reference, false, Vec3::new(0.0, 70.0, 0.0));
        assert!(
            upper.iter().flatten().any(|v| *v > 0.001),
            "actual night stars must be visible"
        );
        for height in [-100.0, -70.0, -68.0, -66.0, -64.0, -62.0, 70.0] {
            fixture.oracle = false;
            let current = fixture.draw(sun, reference, false, Vec3::new(0.0, height, 0.0));
            fixture.oracle = true;
            let source = fixture.draw(sun, reference, false, Vec3::new(0.0, height, 0.0));
            for (a, b) in current.iter().zip(source) {
                for i in 0..3 {
                    assert!(
                        (a[i] - b[i]).abs() < 0.00002,
                        "reference star source/consumer mismatch at height{height}: {a:?}/{b:?}"
                    );
                }
            }
            if height <= -70.0 {
                assert!(
                    current.iter().flatten().all(|v| *v == 0.0),
                    "reference stars below bedrock must be zero"
                );
            }
        }
        fixture.oracle = false;
    }
}

fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f32::from(bits & 1023);
    if exponent == 0 {
        sign * fraction * 2.0f32.powi(-24)
    } else {
        assert_ne!(exponent, 31, "nonfinite celestial output");
        sign * (1.0 + fraction / 1024.0) * 2.0f32.powi(exponent - 15)
    }
}

fn centroid(pixels: &[[f32; 3]]) -> ([f64; 2], f64) {
    let luminance: Vec<f64> = pixels
        .iter()
        .map(|pixel| {
            pixel
                .iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(value, weight)| f64::from(*value) * weight)
                .sum()
        })
        .collect();
    let peak = luminance.iter().copied().fold(0.0, f64::max);
    assert!(peak > 4.0, "actual Sun texture/gain must reach HDR output");
    let mut xy = [0.0; 2];
    let mut weight = 0.0;
    for (i, value) in luminance.into_iter().enumerate() {
        if value >= peak * 0.75 {
            xy[0] += value * (f64::from(i as u32 % SIZE) + 0.5);
            xy[1] += value * (f64::from(i as u32 / SIZE) + 0.5);
            weight += value;
        }
    }
    (xy.map(|component| component / weight), peak)
}

#[test]
fn gpu_production_jg_sun_core_aligns_enhanced_and_preserves_reference_projection() {
    let measured = super::measured_core();
    assert!(measured[0] < 256.0 && measured[1] < 256.0);
    let mut fixture = Fixture::new();
    for sun in [
        Vec3::new(0.3, 0.7, -0.6).normalize(),
        Vec3::new(-0.85, 0.15, 0.45).normalize(),
        Vec3::Y,
        Vec3::new(0.12, 0.984, 0.1).normalize(),
    ] {
        let mut near = None;
        for position in [
            Vec3::ZERO,
            Vec3::new(-617.5, 37.0, -2015.5),
            Vec3::new(32_000.0, 70.0, -32_000.0),
        ] {
            let enhanced = fixture.draw(sun, 0.0, false, position);
            if let Some(near) = &near {
                assert_eq!(&enhanced, near, "sky basis rays must be origin-independent");
            } else {
                near = Some(enhanced.clone());
            }
            let (core, peak) = centroid(&enhanced);
            let center = projected_sun(sun);
            assert!(
                (core[0] - center[0]).hypot(core[1] - center[1]) < 0.75,
                "Sun{sun:?}: enhanced principal core{core:?} must align with lighting"
            );
            let old = fixture.draw(sun, 0.0, true, position);
            let (old_core, old_peak) = centroid(&old);
            assert!(
                (old_core[0] - center[0]).hypot(old_core[1] - center[1]) > 60.0,
                "the fixture must detect the previous baked core offset"
            );
            assert!((peak - old_peak).abs() < 0.03, "display gain is unchanged");
            for reference in [-1.0, 1.0] {
                let current = fixture.draw(sun, reference, false, position);
                let legacy = fixture.draw(sun, reference, true, position);
                assert_eq!(
                    current, legacy,
                    "source reference{reference} projection must be pixel-exact"
                );
            }
            eprintln!(
                "Sun{sun:?} eye{position:?}: core{core:?} peak{peak:.5} legacy core{old_core:?}"
            );
        }
    }
}
