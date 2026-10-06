//! Independent closed-form water optics and production surface sampling.
use super::denoise::{device, draw};

const PRELUDE: &str = r#"
const RAY_PI:f32=3.14159265359;
fn ray_basis(n:vec3f)->mat3x3f {
 let axis=select(vec3f(0.0,1.0,0.0),vec3f(1.0,0.0,0.0),abs(n.y)>0.9);
 let tangent=normalize(cross(axis,n));return mat3x3f(tangent,cross(n,tangent),n);
}
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
"#;

const FIXTURE: &str = r#"
fn random_fixture(state:ptr<function,u32>)->f32 {
 *state=(*state)*747796405u+2891336453u;
 let word=(((*state)>>(((*state)>>28u)+4u))^(*state))*277803737u;
 return f32((word>>22u)^word)/4294967296.0;
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let row=u32(pixel.x);let n=vec3f(0.0,1.0,0.0);
 let entering=RayWaterInterface(n,n,0.0,1.0,RAY_WATER_IOR);
 let leaving=RayWaterInterface(-n,n,0.0,RAY_WATER_IOR,1.0);
 if row==0u {
  return vec4f(ray_water_fresnel(1.0,1.0,RAY_WATER_IOR),
   ray_water_fresnel(0.5,1.0,RAY_WATER_IOR),ray_water_fresnel(0.5,RAY_WATER_IOR,1.0),1.0);
 }
 if row==1u {
  let sample=ray_water_sample(entering,normalize(vec3f(0.6,-0.8,0.0)),vec3f(0.0,0.0,0.99));
  return vec4f(sample.direction,sample.weight.x);
 }
 if row==2u {
  let sample=ray_water_sample(leaving,normalize(vec3f(0.8660254,0.5,0.0)),vec3f(0.0,0.0,0.99));
  return vec4f(sample.direction,sample.weight.x);
 }
 if row==3u||row==4u {return vec4f(ray_water_beer(select(2.0,8.0,row==4u)),1.0);}
 if row==5u {
  let inside=ray_water_sample(entering,-n,vec3f(0.2,0.4,0.99));
  let opposite=RayWaterInterface(n,-n,0.0,RAY_WATER_IOR,1.0);
  let outside=ray_water_sample(opposite,inside.direction,vec3f(0.3,0.6,0.99));
  return vec4f(outside.direction,inside.weight.x*outside.weight.x);
 }
 if row==6u||row==7u {
  let boundary=RayWaterInterface(n,n,select(0.12,0.55,row==7u),1.0,RAY_WATER_IOR);
  let incoming=normalize(vec3f(0.6,-0.8,0.0));
  var state=1991u;var error=0.0;var mean=0.0;var transmitted=0.0;
  for(var i=0u;i<4096u;i++) {
   let xi=vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state));
   let sample=ray_water_sample(boundary,incoming,xi);
   if sample.pdf>0.0 {
    let eval=ray_water_eval(boundary,incoming,sample.direction);
    error=max(error,abs(sample.pdf-eval.pdf)/max(eval.pdf,0.000001));
    error=max(error,abs(sample.weight.x-eval.f.x*abs(dot(n,sample.direction))/eval.pdf));
    mean+=sample.weight.x;
    transmitted+=select(0.0,1.0,sample.transmitted);
   }
  }
  return vec4f(error,mean/4096.0,transmitted/4096.0,1.0);
 }
 if row==8u||row==9u {
  let limit=select(2.0,20.0,row==9u);
  var state=8011u;var survive=vec3f(0.0);var scatter=vec3f(0.0);
  for(var i=0u;i<65536u;i++) {
   let sample=ray_water_medium_sample(limit,vec2f(random_fixture(&state),random_fixture(&state)));
   if sample.event {scatter+=sample.weight;} else {survive+=sample.weight;}
  }
  return vec4f(survive/65536.0,scatter.g/65536.0);
 }
 if row==10u {
  var state=731u;var moment=0.0;var average=0.0;
  for(var i=0u;i<4096u;i++) {
   let ray=ray_water_phase_direction(n,vec2f(random_fixture(&state),random_fixture(&state)));
   moment+=ray.y*ray.y;average+=ray.y;
  }
  return vec4f(moment/4096.0,average/4096.0,ray_water_phase(0.0),ray_water_phase(1.0));
 }
 if row>=15u {
  let case_index=(row-15u)%6u;let rough=row>=21u;
  let cosines=array<f32,6>(1.0,0.5,0.1,1.0,0.9,0.5);let cosine=cosines[case_index];
  let leaving=case_index>=3u;
  let boundary=RayWaterInterface(n,select(n,-n,leaving),select(0.0,0.35,rough),select(1.0,RAY_WATER_IOR,leaving),select(RAY_WATER_IOR,1.0,leaving));
  let incoming=vec3f(sqrt(1.0-cosine*cosine),-cosine,0.0);
  let reflected_color=vec3f(0.8,0.15,2.0);let transmitted_color=vec3f(0.05,1.0,0.3);
  var state=991u+row*1973u;var old_mean=vec3f(0.0);var split_mean=vec3f(0.0);
  for(var i=0u;i<16384u;i++) {
   let sample=ray_water_sample(boundary,incoming,vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state)));
   old_mean+=sample.weight*select(reflected_color,transmitted_color,sample.transmitted);
   let reflected=ray_water_conditional_sample(boundary,incoming,vec2f(random_fixture(&state),random_fixture(&state)),false);
   let transmitted=ray_water_conditional_sample(boundary,incoming,vec2f(random_fixture(&state),random_fixture(&state)),true);
   split_mean+=reflected.weight*reflected_color+transmitted.weight*transmitted_color;
  }
  old_mean/=16384.0;split_mean/=16384.0;
  let error=max(max(abs(old_mean.x-split_mean.x),abs(old_mean.y-split_mean.y)),abs(old_mean.z-split_mean.z));
  return vec4f(error,split_mean);
 }
 if row>=12u {
  let axes=array<vec3f,3>(vec3f(0.0,1.0,0.0),normalize(vec3f(0.7,0.02,-0.3)),normalize(vec3f(-0.23,0.77,0.1)));
  let sun=axes[row-12u];let sampled=ray_water_sun_direction(sun,vec2f(1.0,0.99999994));
  let pdf=ray_water_sun_cone_pdf();
  let radiance=vec3f(1.0,0.6,0.3)/(RAY_PI*sin(RAY_WATER_SUN_RADIUS)*sin(RAY_WATER_SUN_RADIUS));
  return vec4f(radiance/pdf,length(sampled));
 }
 let sun=normalize(vec3f(0.2,0.9,0.1));
 let ray=ray_water_sun_direction(sun,vec2f(0.7,0.6));
 let solar=vec3f(1.0,0.6,0.3);
 let emission=ray_water_sun_emission(sun,solar,ray);
 let angular=RAY_PI*sin(RAY_WATER_SUN_RADIUS)*sin(RAY_WATER_SUN_RADIUS);
 return vec4f(emission*angular,ray_water_sun_pdf(sun,ray));
}
"#;

fn source() -> String {
    let optics = include_str!("../water.wgsl")
        .split("fn ray_water_is")
        .next()
        .unwrap();
    format!("{PRELUDE}\n{optics}\n{FIXTURE}")
}

#[test]
fn production_water_optics_fixture_validates() {
    // Also compile the actual wrapper against the common production triangle
    // ABI and shared raster wave spectrum; pure-math-only validation would
    // miss a flags/normal/clock integration error at the real interface.
    let declarations = include_str!("../intersection.wgsl")
        .split("@group(0)")
        .next()
        .unwrap();
    let wrapper = format!(
        "{PRELUDE}\n{declarations}\nstruct WaterFrame {{water:vec4f}};\nvar<private> ray_frame:WaterFrame;\nvar<private> ray_triangles:array<RayTriangle,1>;\nfn ray_triangle_at(index:u32)->RayTriangle{{return ray_triangles[index];}}\n{}\n{}\n{FIXTURE}",
        include_str!("../../water/waves.wgsl"),
        include_str!("../water.wgsl"),
    );
    for source in [source(), wrapper] {
        let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

fn fresnel(cosine: f64, from: f64, to: f64) -> f64 {
    let transmitted_sine_squared = (from / to).powi(2) * (1.0 - cosine * cosine);
    if transmitted_sine_squared >= 1.0 {
        return 1.0;
    }
    let transmitted = (1.0 - transmitted_sine_squared).sqrt();
    let perpendicular = (from * cosine - to * transmitted) / (from * cosine + to * transmitted);
    let parallel = (to * cosine - from * transmitted) / (to * cosine + from * transmitted);
    (perpendicular * perpendicular + parallel * parallel) / 2.0
}

#[test]
fn gpu_water_fresnel_snell_tir_beer_and_spectral_free_flights() {
    let (device, queue) = device();
    let rows = draw(&device, &queue, &source(), 27, 1, &[], &[]);
    let ior = 1.333f64;
    for (actual, expected) in
        rows[0][..3]
            .iter()
            .zip([fresnel(1.0, 1.0, ior), fresnel(0.5, 1.0, ior), 1.0])
    {
        assert!((f64::from(*actual) - expected).abs() < 1e-6);
    }
    assert!((rows[1][0] - 0.6 / ior as f32).abs() < 1e-6);
    assert!(rows[1][1] < -0.89, "Snell bends toward the normal");
    assert!((rows[1][3] - (1.0 / (ior * ior)) as f32).abs() < 1e-6);
    assert!(
        rows[2][1] < 0.0 && (rows[2][3] - 1.0).abs() < 1e-6,
        "TIR reflects with unit weight"
    );
    let extinction = [0.3406803136f64, 0.0579, 0.0125513234];
    for (row, distance) in [(3, 2.0), (4, 8.0)] {
        for (channel, sigma) in extinction.into_iter().enumerate() {
            assert!((f64::from(rows[row][channel]) - (-sigma * distance).exp()).abs() < 2e-6);
        }
    }
    assert!(
        rows[5][1] < -0.999 && (rows[5][3] - 1.0).abs() < 1e-6,
        "entry/exit eta factors cancel: {:?}",
        rows[5]
    );
    for row in [6, 7] {
        assert!(
            rows[row][0] < 0.0001,
            "BSDF sample must match evaluator/pdf: {:?}",
            rows[row]
        );
        assert!(rows[row][1] > 0.3 && rows[row][1] < 0.65);
        assert!(rows[row][2] > 0.7 && rows[row][2] < 0.999);
    }
    for (row, distance) in [(8, 2.0), (9, 20.0)] {
        for (channel, sigma) in extinction.into_iter().enumerate() {
            assert!(
                (f64::from(rows[row][channel]) - (-sigma * distance).exp()).abs() < 0.015,
                "mixture free-flight survival must preserve Beer RGB: {:?}",
                rows[row]
            );
        }
        let expected_scatter = 0.0014 / extinction[1] * (1.0 - (-extinction[1] * distance).exp());
        assert!((f64::from(rows[row][3]) - expected_scatter).abs() < 0.002);
    }
    assert!(
        (rows[10][0] - 0.4).abs() < 0.025 && rows[10][1].abs() < 0.035,
        "molecular phase sampling has analytic second moment 2/5"
    );
    assert!((rows[10][2] - 3.0 / (16.0 * std::f32::consts::PI)).abs() < 1e-6);
    assert!((rows[10][3] - 3.0 / (8.0 * std::f32::consts::PI)).abs() < 1e-6);
    for (actual, expected) in rows[11][..3].iter().zip([1.0, 0.6, 0.3]) {
        assert!((*actual - expected).abs() < 1e-6);
    }
    for (case, row) in rows[15..27].iter().enumerate() {
        assert!(
            row[0] < 0.025,
            "conditional split must match old Bernoulli mean without boosting: case{case}: {row:?}"
        );
        if case < 6 {
            let cosine = [1.0, 0.5, 0.1, 1.0, 0.9, 0.5][case];
            let (from, to) = if case < 3 { (1.0, ior) } else { (ior, 1.0) };
            let reflectance = fresnel(cosine, from, to);
            for (value, (reflected, transmitted)) in row[1..]
                .iter()
                .zip([0.8, 0.15, 2.0].into_iter().zip([0.05, 1.0, 0.3]))
            {
                let expected = reflectance * reflected
                    + (1.0 - reflectance) * (from / to).powi(2) * transmitted;
                assert!(
                    (f64::from(*value) - expected).abs() < 0.0005,
                    "smooth split closed form including grazing and TIR: case{case}: {row:?}"
                );
            }
        }
    }
    for row in &rows[12..15] {
        assert!(row.iter().all(|v| v.is_finite()) && (row[3] - 1.0).abs() < 0.000002);
        let ratio = 2.0 * (1.0 - f64::from(0.00465f32).cos()) / f64::from(0.00465f32).sin().powi(2);
        for (actual, solar) in row[..3].iter().zip([1.0, 0.6, 0.3]) {
            assert!(
                (f64::from(*actual) - solar * ratio).abs() < 0.006,
                "sampled solar endpoints preserve finite analytic NEE across axes: {row:?}"
            );
        }
    }
    assert!(
        rows[11][3] > 10000.0,
        "finite disc has an explicit nonzero solid-angle PDF"
    );
}

#[path = "water/volume.rs"]
mod volume;

#[path = "water/absorption.rs"]
mod absorption;

#[path = "water/interface.rs"]
mod interface;

#[path = "water/caustics.rs"]
mod caustics;

#[path = "water/coast.rs"]
mod coast;

#[path = "water/guide_lobes.rs"]
mod guide_lobes;
