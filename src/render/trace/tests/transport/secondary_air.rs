//! Actual event/shadow helpers: analytic air, retained cloud density and budget.
use super::super::denoise::{device, draw};
use wgpu::util::DeviceExt;

const FIXTURE: &str = r#"
const RAY_PI:f32=3.14159265359;
struct Frame {parameters:vec4f,sun:vec4f,cloud:vec4f};
@group(0) @binding(0) var<uniform> ray_frame:Frame;
var<private> ray_rng:u32;var<private> rng_count:u32;var<private> density_count:u32;
fn random()->f32 {
 rng_count++;ray_rng=ray_rng*747796405u+2891336453u;
 let word=((ray_rng>>((ray_rng>>28u)+4u))^ray_rng)*277803737u;
 return f32((word>>22u)^word)/4294967296.0;
}
fn bg_cloud_density(p:vec3f,coverage:f32,drift:vec2f)->f32 {
 density_count++;
 if ray_frame.parameters.y==2.0 {
  return select(0.0,0.03,p.y>BG_CLOUD_BOTTOM&&p.y<BG_CLOUD_TOP);
 }
 return source_cloud_density(p,coverage,drift);
}
fn medium_density(p:vec3f)->f32 {
 return ray_frame.parameters.x*exp(-max(p.y-20.0,0.0)/90.0)+bg_cloud_density(p,ray_frame.cloud.x,ray_frame.cloud.yz);
}
fn medium_anisotropy(p:vec3f)->f32 {return 0.0;}
fn medium_phase(cosine:f32,g:f32)->f32 {return 1.0;}
fn medium_direction(incoming:vec3f,g:f32)->vec3f {return incoming;}
fn sun_light(p:vec3f)->vec3f {return vec3f(0.0);}
fn ray_water_component(value:vec3f,water_scattered:bool)->vec3f {return value;}
fn transport_state(p:vec3f,direction:vec3f,sky:f32,limit:f32,primary:bool,footprint:vec4f,initial_depth:u32,pdf:f32,eta:f32)->vec3f {return vec3f(0.0);}
struct Output {@builtin(position) position:vec4f};
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Output {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Output(vec4f(xy*2.0-1.0,0.0,1.0));
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 ray_rng=u32(pixel.x)*1973u+911u;rng_count=0u;density_count=0u;
 if ray_frame.parameters.y>0.0 {
  var origin=vec3f(f32(u32(pixel.x)%8u)*128.0,100.0,f32(u32(pixel.x)/8u)*128.0);
  var direction=normalize(vec3f(-0.55,0.65,-0.52));var limit=512.0;
  if ray_frame.parameters.y==2.0 {origin=vec3f(0.0);direction=vec3f(0.0,1.0,0.0);limit=ray_frame.parameters.z;}
  let depth=ray_segment_optical_depth(origin,direction,limit);let cost=density_count;
  let interval=bg_cloud_interval(origin,direction,limit);var reference=ray_air_depth(origin.y,direction.y,limit);
  if interval.y>interval.x {
   let step=(interval.y-interval.x)/512.0;
   for(var i=0u;i<512u;i++) {reference+=bg_cloud_density(origin+direction*(interval.x+(f32(i)+0.5)*step),ray_frame.cloud.x,ray_frame.cloud.yz)*step;}
  }
  var old=0.0;
  for(var i=0u;i<16u;i++) {old+=medium_density(origin+direction*((f32(i)+0.5)*limit/16.0))*limit/16.0;}
  return vec4f(depth,reference,old,f32(cost));
 }
 var input=vec3f(10.0,0.0,30.0);
 switch u32(ray_frame.parameters.z) {
  case 1u:{input=vec3f(110.0,0.0,30.0);}
  case 2u:{input=vec3f(30.0,1.0,90.0);}
  case 3u:{input=vec3f(160.0,-1.0,90.0);}
  case 4u:{input=vec3f(-10.0,1.0,60.0);}
  case 5u:{input=vec3f(50.0,-1.0,60.0);}
  default:{}
 }
 let direction=vec3f(sqrt(max(0.0,1.0-input.y*input.y)),input.y,0.0);
 var events=0.0;var half=0.0;
 for(var i=0u;i<64u;i++) {
  let event=medium_event(vec3f(0.0,input.x,0.0),direction,input.z);
  if event<input.z {events+=1.0;}
  if event<input.z*0.5 {half+=1.0;}
 }
 return vec4f(events/64.0,half/64.0,f32(rng_count)/64.0,f32(density_count));
}
"#;

fn source() -> String {
    let transport = format!(
        "{}\n{}",
        include_str!("../../transport.wgsl"),
        include_str!("../../paired.wgsl")
    );
    let start = transport.find("fn medium_event(").unwrap();
    let end = transport.find("fn medium_anisotropy(").unwrap();
    let cloud = crate::render::sky::CLOUD_SHADER
        .replace("fn bg_cloud_density(", "fn source_cloud_density(");
    format!(
        "{cloud}\n{FIXTURE}\n{}\n{}",
        &transport[start..end],
        include_str!("../../medium.wgsl")
    )
}

#[test]
fn secondary_air_and_shadow_fixture_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(&source()).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

fn run(density: f32, mode: f32, case: f32, width: u32) -> Vec<[f32; 4]> {
    let (device, queue) = device();
    let data = [
        density, mode, case, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0,
    ];
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
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

fn air_depth(height: f64, vertical: f64, distance: f64, sigma: f64) -> f64 {
    // Independent numerical integral, distinct from production piecewise formulas.
    let step = distance / 8192.0;
    (0..8192)
        .map(|i| {
            sigma
                * (-(height + vertical * (i as f64 + 0.5) * step - 20.0).max(0.0) / 90.0).exp()
                * step
        })
        .sum()
}

#[test]
fn gpu_secondary_cloud_free_air_preserves_event_cdf_without_rejection_work() {
    for (case, (height, vertical, distance)) in [
        (10.0, 0.0, 30.0),
        (110.0, 0.0, 30.0),
        (30.0, 1.0, 90.0),
        (160.0, -1.0, 90.0),
        (-10.0, 1.0, 60.0),
        (50.0, -1.0, 60.0),
    ]
    .into_iter()
    .enumerate()
    {
        let pixels = run(0.01, 0.0, case as f32, 1024);
        let average: [f64; 4] = std::array::from_fn(|channel| {
            pixels
                .iter()
                .map(|pixel| f64::from(pixel[channel]))
                .sum::<f64>()
                / pixels.len() as f64
        });
        for (channel, limit) in [(0, distance), (1, distance * 0.5)] {
            let expected = -(-air_depth(height, vertical, limit, f64::from(0.01f32))).exp_m1();
            assert!(
                (average[channel] - expected).abs() < 0.007,
                "eventCDF case{case} limit{limit}: actual{average:?} expected{expected}"
            );
        }
        assert_eq!(
            average[2], 1.0,
            "one optical-depth variate, without rejection loop"
        );
        assert_eq!(
            average[3], 0.0,
            "cloud-free segment never evaluates 3D density"
        );
    }
    let vacuum = run(0.0, 0.0, 0.0, 32);
    assert!(
        vacuum.iter().all(|pixel| *pixel == [0.0; 4]),
        "vacuum has zero events and no sampling work"
    );
}

#[test]
fn gpu_shadow_depth_keeps_clipped_cloud_mass_and_exact_air() {
    for limit in [192.5, 205.0, 250.0, 300.0] {
        let pixel = run(0.01, 2.0, limit, 1)[0];
        let expected = air_depth(0.0, 1.0, f64::from(limit), f64::from(0.01f32))
            + f64::from((limit - 192.0).clamp(0.0, 60.0)) * f64::from(0.03f32);
        assert!(
            (f64::from(pixel[0]) - expected).abs() < 0.00003,
            "clipped cloud mass {limit}: {pixel:?} expected{expected}"
        );
        assert!(pixel[3] >= 1.0 && pixel[3] <= 16.0);
    }
    let pixels = run(0.0002, 1.0, 0.0, 64);
    let new_error = pixels
        .iter()
        .map(|p| (f64::from(-p[0]).exp() - f64::from(-p[1]).exp()).abs())
        .sum::<f64>()
        / pixels.len() as f64;
    let old_error = pixels
        .iter()
        .map(|p| (f64::from(-p[2]).exp() - f64::from(-p[1]).exp()).abs())
        .sum::<f64>()
        / pixels.len() as f64;
    eprintln!("actual-cloud shadow mean transmission error new={new_error:.6}, old={old_error:.6}");
    assert!(
        new_error <= old_error + 0.005,
        "bounded cloud quadrature must not lose model accuracy: new{new_error} old{old_error}"
    );
    assert!(
        pixels.iter().all(|p| p[3] == 3.0),
        "only three in-slab samples, not sixteen full segment evaluations"
    );
}
