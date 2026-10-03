use glam::Vec3;

use super::daylight::Atmosphere;
use super::{Camera, DEPTH_FORMAT};

pub(crate) fn create_sky_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("atmospheric sky shader"),
        source: wgpu::ShaderSource::Wgsl(SKY_SHADER.into()),
    });
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sky camera basis"),
        size: 96,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sky camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky camera bind group"),
        layout: &camera_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky pipeline layout"),
        bind_group_layouts: &[Some(&camera_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("atmospheric sky pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, camera_buffer, camera_group)
}

/// Camera basis packed as three aligned vec4 uniforms. The sun itself stays in
/// world space; looking away from it cannot leave a screen-fixed bright disc.
pub(crate) fn sky_camera_data(
    camera: Camera,
    width: u32,
    height: u32,
    atmosphere: Atmosphere,
) -> [f32; 24] {
    let forward = camera.direction();
    let right = Vec3::new(-camera.yaw.sin(), 0.0, camera.yaw.cos());
    let up = right.cross(forward).normalize();
    let vertical = (camera.fov_y_radians * 0.5).tan();
    let horizontal = vertical * width as f32 / height.max(1) as f32;
    [
        forward.x,
        forward.y,
        forward.z,
        atmosphere.cloud,
        right.x,
        right.y,
        right.z,
        horizontal,
        up.x,
        up.y,
        up.z,
        vertical,
        atmosphere.sun.x,
        atmosphere.sun.y,
        atmosphere.sun.z,
        atmosphere.strength,
        atmosphere.horizon.x,
        atmosphere.horizon.y,
        atmosphere.horizon.z,
        atmosphere.drift[0],
        atmosphere.zenith.x,
        atmosphere.zenith.y,
        atmosphere.zenith.z,
        atmosphere.drift[1],
    ]
}
const SKY_SHADER: &str = r#"
struct SkyCamera {
    forward: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    sun: vec4f,
    horizon: vec4f,
    zenith: vec4f,
};
@group(0) @binding(0) var<uniform> sky_camera: SkyCamera;
struct SkyVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertex {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var output: SkyVertex;
    let p = positions[index];
    output.position = vec4<f32>(p, 0.99999, 1.0);
    output.uv = p * 0.5 + vec2<f32>(0.5);
    return output;
}
fn sky_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn sky_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let curve = f * f * (vec2<f32>(3.0) - 2.0 * f);
    let low = mix(sky_hash(cell), sky_hash(cell + vec2<f32>(1.0, 0.0)), curve.x);
    let high = mix(sky_hash(cell + vec2<f32>(0.0, 1.0)), sky_hash(cell + vec2<f32>(1.0, 1.0)), curve.x);
    return mix(low, high, curve.y);
}
@fragment fn fs_main(input: SkyVertex) -> @location(0) vec4<f32> {
    let ndc = input.uv * 2.0 - vec2<f32>(1.0);
    let ray = normalize(
        sky_camera.forward.xyz
        + sky_camera.right.xyz * ndc.x * sky_camera.right.w
        + sky_camera.up.xyz * ndc.y * sky_camera.up.w
    );
    let horizon = sky_camera.horizon.xyz;
    let zenith = sky_camera.zenith.xyz;
    var color = mix(horizon, zenith, smoothstep(-0.08, 0.86, ray.y));
    let sun_direction = normalize(sky_camera.sun.xyz);
    let alignment = dot(ray, sun_direction);
    let haze = pow(max(alignment, 0.0), 10.0) * (1.0 - smoothstep(0.1, 0.75, ray.y));
    color = mix(color, vec3<f32>(0.95, 0.72, 0.54), haze * 0.22);
    let cloud_coordinates = ray.xz / max(ray.y, 0.10) * 8.0 + vec2f(sky_camera.horizon.w, sky_camera.zenith.w);
    let cloud_noise = sky_noise(cloud_coordinates * 0.45) * 0.68
        + sky_noise(cloud_coordinates * 0.90) * 0.32;
    let cloud_edge = max(0.075, 0.5 * fwidth(cloud_noise));
    let cover = sky_camera.forward.w;
    let threshold = mix(0.625, 0.25, cover);
    let cloud = smoothstep(threshold - cloud_edge, threshold + cloud_edge, cloud_noise)
        * smoothstep(0.02, mix(0.28, 0.08, cover), ray.y) * mix(0.54, 0.97, cover);
    color = mix(color, mix(vec3f(0.025, 0.035, 0.06), vec3f(0.92, 0.94, 0.94), sky_camera.sun.w) * mix(1.0, 0.52, cover), cloud);
    let glow = smoothstep(0.88, 0.997, alignment);
    let disc = smoothstep(0.9990, 0.99955, alignment);
    color = mix(color, vec3<f32>(1.0, 0.82, 0.55), glow * 0.28 * smoothstep(-0.08, 0.08, sun_direction.y));
    color = mix(color, vec3<f32>(5.0, 4.4, 3.2), disc * smoothstep(-0.04, 0.02, sun_direction.y) * (1.0 - cover));
    let night = 1.0 - smoothstep(0.04, 0.35, sky_camera.sun.w);
    let star_cell = floor(ray.xz / max(ray.y, 0.12) * 120.0);
    let stars = smoothstep(0.997, 1.0, sky_hash(star_cell)) * smoothstep(0.08, 0.35, ray.y);
    color += vec3f(0.7, 0.8, 1.0) * stars * night * (1.0 - cloud);
    let moon = smoothstep(0.9992, 0.99965, dot(ray, -sun_direction));
    color = mix(color, vec3f(0.55, 0.65, 0.85), moon * night);
    // A storm has an unbroken cloud ceiling. Noise shades its surface instead
    // of cutting blue holes into it, including at the horizon and sun/moon.
    let overcast = smoothstep(0.75, 1.0, cover);
    // The planar cloud projection is only meaningful above the horizon.
    // Fade its contrast out before it can form a curtain below unloaded terrain.
    let storm_sky = mix(horizon, zenith, smoothstep(-0.08, 0.86, ray.y))
        * mix(1.0, 0.85 + 0.30 * cloud_noise, smoothstep(0.02, 0.20, ray.y));
    color = mix(color, storm_sky, overcast);
    return vec4<f32>(color, 1.0);
}
"#;

#[cfg(test)]
#[path = "sky/tests.rs"]
mod tests;
