//! Actual coast camera, perspective, low-resolution sample centers and TAA jitter.
//! The old water distance comes from an independent inverse-VP plane hit.
use super::*;
use crate::render::{Camera, post::temporal, visibility};
use glam::Vec2;

const FIXTURE: &str = r#"
struct Case { previous:mat4x4f,eye:vec4f,position:vec4f,normal_class:vec4f,
 pixel_size:vec4f,old_geometry:vec4f,data:vec4f };
@group(0) @binding(0) var<storage,read> cases:array<Case>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 let c=cases[u32(p.x)];
 let accepted=ray_water_history_compatible(c.data.x,c.old_geometry,c.normal_class.xyz,c.normal_class.w,
  c.position.xyz,c.previous,c.eye.xyz,c.pixel_size.xy,c.pixel_size.zw,c.data.y);
 let radial=ray_history_compatible(c.data.x,length(c.position.xyz-c.eye.xyz),c.old_geometry,
  c.normal_class.xyz,abs(c.normal_class.w),true,c.data.y);
 let ndc=(c.pixel_size.xy+0.5)/c.pixel_size.zw*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
 let r0=vec3f(c.previous[0].x,c.previous[1].x,c.previous[2].x);
 let r1=vec3f(c.previous[0].y,c.previous[1].y,c.previous[2].y);
 let r3=vec3f(c.previous[0].w,c.previous[1].w,c.previous[2].w);
 var d=normalize(cross(r0-ndc.x*r3,r1-ndc.y*r3));
 if dot(d,c.position.xyz-c.eye.xyz)<0.0 {d=-d;}
 let residual=abs(dot(c.position.xyz-c.eye.xyz-d*c.data.x,c.normal_class.xyz));
 return vec4f(select(0.0,1.0,accepted),select(0.0,1.0,radial),
  select(1.0,c.old_geometry.w+1.0,accepted),residual);
}
"#;

struct Sample {
    words: [f32; 40],
    valid: bool,
    label: String,
}
fn direction(matrix: Mat4, eye: Vec3, pixel: Vec2) -> Vec3 {
    let ndc = (pixel + Vec2::splat(0.5)) / Vec2::new(1280.0, 800.0) * Vec2::new(2.0, -2.0)
        + Vec2::new(-1.0, 1.0);
    let inverse = (matrix * Mat4::from_translation(eye)).inverse();
    let far = inverse * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
    (far.truncate() / far.w).normalize()
}
fn sample(camera: Camera, stride: i32, pixel: Vec2, frame: u32) -> Option<Sample> {
    let vp = visibility::view_projection(camera, 1280, 800);
    let current = temporal::jitter_matrix(vp, temporal::jitter(frame), 1280, 800);
    let previous = temporal::jitter_matrix(vp, temporal::jitter(frame - 1), 1280, 800);
    let ray = direction(current, camera.position, pixel);
    let distance = (17.0 - camera.position.y) / ray.y;
    if distance <= 0.0 || distance > 2000.0 {
        return None;
    }
    let position = camera.position + ray * distance;
    let projected = previous * position.extend(1.0);
    let uv =
        projected.truncate().truncate() / projected.w * Vec2::new(0.5, -0.5) + Vec2::splat(0.5);
    let low_size = Vec2::new(1280.0 / stride as f32, 800.0 / stride as f32);
    let hp = (uv * low_size)
        .floor()
        .clamp(Vec2::ZERO, low_size - Vec2::ONE);
    let previous_pixel = hp * stride as f32 + Vec2::splat((stride / 2) as f32);
    let old_ray = direction(previous, camera.position, previous_pixel);
    let raw_depth = (17.0 - camera.position.y) / old_ray.y;
    if raw_depth <= 0.0 {
        return None;
    }
    // Match the half-float HDR history alpha's normal-range quantization.
    let step = 2.0f32.powf(raw_depth.log2().floor() - 10.0);
    let old_depth = (raw_depth / step).round() * step;
    let mut words = [0.0; 40];
    words[..16].copy_from_slice(&previous.to_cols_array());
    words[16..19].copy_from_slice(&camera.position.to_array());
    words[20..23].copy_from_slice(&position.to_array());
    words[24..28].copy_from_slice(&[0.0, 1.0, 0.0, -3.12]);
    words[28..32].copy_from_slice(&[previous_pixel.x, previous_pixel.y, 1280.0, 800.0]);
    words[32..36].copy_from_slice(&[0.0, 1.0, -3.12, 8.0]);
    words[36..40].copy_from_slice(&[old_depth, 0.25, distance, 0.0]);
    Some(Sample {
        words,
        valid: true,
        label: format!(
            "stride={stride} pixel={pixel:?} frame={frame} depth={distance}/{old_depth}"
        ),
    })
}

#[test]
fn gpu_water_history_accepts_coast_grazing_plane_across_jitter_and_rejects_disocclusion() {
    let eye = Vec3::new(-617.5, 37.0, -2015.5);
    let target = Vec3::new(-655.5, 19.0, -2047.5);
    let view = (target - eye).normalize();
    let camera = Camera {
        position: eye,
        yaw: view.z.atan2(view.x),
        pitch: view.y.asin(),
        fov_y_radians: 70.0f32.to_radians(),
    };
    let mut cases = Vec::new();
    for translation in [
        Vec3::ZERO,
        Vec3::new(-16_000.0, 0.0, 32_000.0),
        Vec3::new(16_000.0, 0.0, -32_000.0),
    ] {
        let camera = Camera {
            position: eye + translation,
            ..camera
        };
        for stride in [2, 4, 8] {
            for frame in 1..=8 {
                for y in [238, 258, 278, 298, 338, 398, 458, 618] {
                    for x in [642, 902, 1154] {
                        let pixel = Vec2::new(
                            ((x / stride) * stride + stride / 2) as f32,
                            ((y / stride) * stride + stride / 2) as f32,
                        );
                        if let Some(case) = sample(camera, stride, pixel, frame) {
                            cases.push(case);
                        }
                    }
                }
            }
        }
    }
    assert!(cases.len() >= 400);
    let valid_count = cases.len();
    let base = sample(camera, 4, Vec2::new(902.0, 278.0), 2).unwrap();
    for invalid in 0..6 {
        let mut words = base.words;
        let label = match invalid {
            0 => {
                words[21] += 1.0;
                "parallel water layer"
            }
            1 => {
                words[33] = -1.0;
                "opposite geometric normal"
            }
            2 => {
                words[34] = -3.5;
                "roughness discontinuity"
            }
            3 => {
                words[35] = 0.0;
                "uninitialized history"
            }
            4 => {
                words[34] = -0.12;
                "raster fallback class"
            }
            _ => {
                words[36] *= 0.5;
                "foreground occluder"
            }
        };
        cases.push(Sample {
            words,
            valid: false,
            label: label.into(),
        });
    }
    let (device, queue) = device();
    let storage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("coast perspective history cases"),
        contents: bytemuck::cast_slice(&cases.iter().map(|c| c.words).collect::<Vec<_>>()),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: storage.as_entire_binding(),
        }],
    });
    let output = draw(
        &device,
        &queue,
        &format!("{HELPERS}\n{FIXTURE}"),
        cases.len() as u32,
        1,
        &[&group],
        &[Some(&layout)],
    );
    let mut legacy_rejected = 0;
    for (case, result) in cases.iter().zip(&output) {
        assert_eq!(result[0] > 0.5, case.valid, "{}: {result:?}", case.label);
        assert_eq!(
            result[2],
            if case.valid { 9.0 } else { 1.0 },
            "{}",
            case.label
        );
        if case.valid && result[1] < 0.5 {
            legacy_rejected += 1;
        }
    }
    eprintln!(
        "coast perspective water history: {valid_count} valid samples preserved; legacy radial check rejected {legacy_rejected}"
    );
    assert!(
        legacy_rejected > valid_count / 5,
        "fixture must expose actual grazing/jitter rejection"
    );
}
