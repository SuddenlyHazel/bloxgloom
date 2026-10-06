//! Analytic extinction and unbiased weak-medium continuation, using production WGSL.
use super::super::denoise::{device, draw};
use wgpu::util::DeviceExt;

const MEDIUM: &str = include_str!("../../medium.wgsl");
const FIXTURE: &str = r#"
const RAY_PI:f32=3.14159265359;
struct Frame {parameters:vec4f,sun:vec4f,cloud:vec4f};
@group(0) @binding(0) var<uniform> ray_frame:Frame;
var<private> ray_rng:u32;
fn random()->f32 {
 ray_rng=ray_rng*747796405u+2891336453u;
 let word=((ray_rng>>((ray_rng>>28u)+4u))^ray_rng)*277803737u;
 return f32((word>>22u)^word)/4294967296.0;
}
fn bg_cloud_interval(origin:vec3f,direction:vec3f,distance:f32)->vec2f {
 return select(vec2f(0.0),vec2f(0.0,distance),ray_frame.parameters.z>0.0);
}
fn medium_density(p:vec3f)->f32 {return ray_frame.parameters.x*exp(-max(p.y-20.0,0.0)/90.0);}
fn medium_anisotropy(p:vec3f)->f32 {return 0.0;}
fn medium_phase(cosine:f32,g:f32)->f32 {return 1.0;}
fn medium_direction(incoming:vec3f,g:f32)->vec3f {return incoming;}
fn sun_light(p:vec3f)->vec3f {return vec3f(0.8,0.9,1.0);}
fn transport(p:vec3f,direction:vec3f,sky:f32)->vec3f {return vec3f(0.2,0.3,0.4);}
struct Output {@builtin(position) position:vec4f};
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Output {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Output(vec4f(xy*2.0-1.0,0.0,1.0));
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 ray_rng=u32(pixel.x)*1973u+911u;
 if ray_frame.parameters.y>0.0 {
  let color=vec3f(0.3,0.5,0.7);let correction=vec3f(0.2,0.4,0.8);
  let tau=ray_frame.parameters.x*30.0;let opacity=ray_exponential_opacity(tau);
  let baseline=correction*exp(-tau)-color*opacity;
  var energy=vec3f(0.0);var continuations=0.0;
  for(var i=0u;i<64u;i++) {
   let result=ray_primary_medium_sample(vec3f(0.0,10.0,0.0),vec3f(1.0,0.0,0.0),30.0,color,correction);
   energy+=(result.correction-baseline)/max(opacity,0.000000001);
   if result.correction.r>baseline.r+0.000001 {continuations+=1.0;}
  }
  return vec4f(energy/64.0,continuations/64.0);
 }
 var input=vec3f(10.0,0.0,30.0);
 switch u32(pixel.x) {
  case 1u:{input=vec3f(110.0,0.0,30.0);}
  case 2u:{input=vec3f(30.0,1.0,180.0);}
  case 3u:{input=vec3f(200.0,-1.0,90.0);}
  case 4u:{input=vec3f(-10.0,1.0,60.0);}
  case 5u:{input=vec3f(50.0,-1.0,60.0);}
  case 6u:{input=vec3f(20.0,1.0,90.0);}
  case 7u:{input=vec3f(20.0,-1.0,90.0);}
  case 8u:{input=vec3f(9000.0,-1.0,9000.0);}
  case 9u:{input=vec3f(21.0,0.00001,300.0);}
  case 10u:{input=vec3f(19.999,0.00001,300.0);}
  default:{}
 }
 let tau=ray_air_depth(input.x,input.y,input.z);
 let optical_sample=ray_negative_log_one_minus(0.73*ray_exponential_opacity(tau));
 let distance=ray_air_distance(input.x,input.y,input.z,optical_sample);
 let direction=vec3f(sqrt(max(0.0,1.0-input.y*input.y)),input.y,0.0);
 let result=ray_primary_medium_sample(vec3f(0.0,input.x,0.0),direction,input.z,vec3f(0.0),vec3f(0.0));
 return vec4f(result.transmission,tau,distance,ray_air_depth(input.x,input.y,distance));
}
"#;

fn source() -> String {
    format!("{FIXTURE}\n{MEDIUM}")
}
#[test]
fn analytic_air_shader_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
fn run(density: f32, statistical: bool, cloud: bool, width: u32) -> Vec<[f32; 4]> {
    let (device, queue) = device();
    let data = [
        density,
        f32::from(statistical),
        f32::from(cloud),
        0.0,
        0.0,
        1.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ];
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("analytic air reference"),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    draw(
        &device,
        &queue,
        &source(),
        width,
        1,
        &[&group],
        &[Some(&layout)],
    )
}
fn reference_depth(height: f64, vertical: f64, distance: f64, sigma: f64) -> f64 {
    if vertical == 0.0 {
        return sigma * (-(height - 20.0).max(0.0) / 90.0).exp() * distance;
    }
    let near = height.min(height + vertical * distance);
    let below = ((20.0 - near) / vertical.abs()).clamp(0.0, distance);
    let above = distance - below;
    sigma * below
        + sigma * (-(near - 20.0).max(0.0) / 90.0).exp() * 90.0 / vertical.abs()
            * (-(-vertical.abs() * above / 90.0).exp_m1())
}
#[test]
fn gpu_primary_air_exact_transmission_and_inverse_cdf_cross_height_boundary() {
    let inputs = [
        (10.0, 0.0, 30.0),
        (110.0, 0.0, 30.0),
        (30.0, 1.0, 180.0),
        (200.0, -1.0, 90.0),
        (-10.0, 1.0, 60.0),
        (50.0, -1.0, 60.0),
        (20.0, 1.0, 90.0),
        (20.0, -1.0, 90.0),
        (9000.0, -1.0, 9000.0),
        (21.0, 0.00001, 300.0),
        (f64::from(19.999f32), 0.00001, 300.0),
    ];
    let pixels = run(0.01, false, false, inputs.len() as u32);
    for (i, (pixel, (height, vertical, distance))) in pixels.iter().zip(inputs).enumerate() {
        let tau = reference_depth(height, vertical, distance, f64::from(0.01f32));
        assert!(
            (f64::from(pixel[0]) - (-tau).exp()).abs() < 0.00002,
            "exact transmission case{i}: {pixel:?} tau={tau}"
        );
        assert!(
            (f64::from(pixel[1]) - tau).abs() < 0.00005,
            "exact optical depth case{i}: {pixel:?} tau={tau}"
        );
        assert!(
            pixel.iter().all(|v| v.is_finite())
                && pixel[2] >= 0.0
                && f64::from(pixel[2]) <= distance
        );
        let target = -(-0.73 * (-(-tau).exp_m1())).ln_1p();
        let sampled = reference_depth(height, vertical, f64::from(pixel[2]), f64::from(0.01f32));
        assert!(
            (sampled - target).abs() < 0.00005,
            "analytic inverseCDF case{i}: {pixel:?} tau(sample)={sampled} target={target}"
        );
        assert!((f64::from(pixel[3]) - target).abs() < 0.00005);
    }
}
#[test]
fn gpu_primary_weak_air_continuation_retains_unbiased_source_energy() {
    for (density, cloud) in [(0.00002f32, false), (0.05, false), (0.00002, true)] {
        let pixels = run(density, true, cloud, 4096);
        let mean = std::array::from_fn::<_, 4, _>(|c| {
            pixels.iter().map(|p| f64::from(p[c])).sum::<f64>() / pixels.len() as f64
        });
        for (channel, expected) in [0.92, 1.104, 1.288].into_iter().enumerate() {
            assert!(
                (mean[channel] - expected).abs() < expected * 0.045,
                "unbiased source energy density={density} cloud={cloud} mean={mean:?}"
            );
        }
        let opacity = -(-(f64::from(density) * 30.0)).exp_m1();
        let expected = (opacity * 32.0).min(1.0);
        assert!(
            (mean[3] - expected).abs() < expected * 0.045,
            "weak media should skip costly continuation without losing energy: {mean:?}, expectedfrequency={expected}"
        );
    }
    let vacuum = run(0.0, true, false, 16);
    assert!(vacuum.iter().all(|p| p.iter().all(|v| *v == 0.0)));
}
