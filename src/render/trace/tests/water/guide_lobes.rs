//! Independent angular energy/variance proofs for guided phase and opaque GGX.
use super::*;
const LOBES: &str = r#"
fn water_connection(d:vec3f,sun:vec3f)->f32 {
 let boundary=RayWaterInterface(vec3f(0.0,-1.0,0.0),vec3f(0.0,1.0,0.0),0.12,RAY_WATER_IOR,1.0);
 return ray_water_eval(boundary,d,sun).f.x*sun.y;
}
@fragment fn fs_main(@builtin(position) p:vec4f)->@location(0) vec4f {
 let row=u32(p.y);let n=vec3f(0.0,1.0,0.0);let incoming=-n;
 let heights=array<f32,3>(1.0,0.5,0.1);let h=heights[row%3u];
 let sun=vec3f(sqrt(1.0-h*h),h,0.0);
 let guide=RayWaterGuide(normalize(-refract(-sun,n,1.0/RAY_WATER_IOR)),cos(ray_water_guide_angle(0.12,h)),true);
 let disabled=RayWaterGuide(n,1.0,false);
 let pbr=bg_decode_pbr(vec4f(0.1,0.0,0.0,0.0),vec3f(1.0),false,true);
 var state=3881u+row*911u+u32(p.x)*1973u;
 var old_mean=0.0;var new_mean=0.0;var old_second=0.0;var new_second=0.0;
 for(var i=0u;i<512u;i++) {
  let old_xi=vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state));
  let new_xi=vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state));
  var old_value=0.0;var new_value=0.0;
  if row<3u||row==6u {
   let old=ray_water_guided_phase(incoming,disabled,old_xi);
   let proposed=ray_water_guided_phase(incoming,guide,new_xi);
   old_value=old.weight*water_connection(old.direction,sun);
   new_value=proposed.weight*water_connection(proposed.direction,sun);
   if row==6u {old_value=old.weight;new_value=proposed.weight;}
  } else {
   let old=ray_water_guided_ggx(n,n,pbr,disabled,old_xi);
   let proposed=ray_water_guided_ggx(n,n,pbr,guide,new_xi);
   old_value=old.weight.x*water_connection(old.direction,sun);
   new_value=proposed.weight.x*water_connection(proposed.direction,sun);
   if row==7u {old_value=old.weight.x;new_value=proposed.weight.x;}
  }
  let old_delta=old_value-old_mean;old_mean+=old_delta/f32(i+1u);old_second+=old_delta*(old_value-old_mean);
  let new_delta=new_value-new_mean;new_mean+=new_delta/f32(i+1u);new_second+=new_delta*(new_value-new_mean);
 }
 return vec4f(old_mean,new_mean,old_second/512.0+old_mean*old_mean,new_second/512.0+new_mean*new_mean);
}
"#;
fn lobe_source() -> String {
    let rng = FIXTURE.split("@fragment fn fs_main").next().unwrap();
    let helpers = include_str!("../../water/caustics.wgsl")
        .split("// Querying")
        .next()
        .unwrap();
    source().replace(
        FIXTURE,
        &format!(
            "{}\n{rng}\n{helpers}\n{}\n{LOBES}",
            include_str!("../../../material/pbr.wgsl"),
            include_str!("../../water/ggx.wgsl")
        ),
    )
}
#[test]
fn gpu_water_phase_and_opaque_ggx_guides_preserve_energy_and_reduce_solar_variance() {
    let (device, queue) = device();
    let pixels = draw(&device, &queue, &lobe_source(), 8192, 8, &[], &[]);
    let mut rows = [[0.0f64; 4]; 8];
    for (row, line) in pixels.chunks_exact(8192).enumerate() {
        for pixel in line {
            for (sum, value) in rows[row].iter_mut().zip(pixel) {
                *sum += f64::from(*value) / 8192.0;
            }
        }
        rows[row][2] -= rows[row][0] * rows[row][0];
        rows[row][3] -= rows[row][1] * rows[row][1];
    }
    for (index, row) in rows[..6].iter().enumerate() {
        let standard_error = ((row[2] + row[3]) / 4194304.0).sqrt();
        println!(
            "guided family={} solar_cosine={} old_mean={} new_mean={} old_variance={} new_variance={} stderr={}",
            if index < 3 { "Rayleigh" } else { "GGX" },
            [1.0, 0.5, 0.1][index % 3],
            row[0],
            row[1],
            row[2],
            row[3],
            standard_error
        );
        assert!(row.iter().all(|v| v.is_finite()) && row[0] > 0.0 && row[1] > 0.0);
        assert!(
            (row[0] - row[1]).abs() < 5.0 * standard_error + 0.00002,
            "full-mixture weighting preserves actual angular energy: {row:?}"
        );
        assert!(
            row[3] < row[2] * 0.25,
            "guide must materially reduce variance in both omitted families: {row:?}"
        );
    }
    assert!(
        (rows[6][0] - 1.0).abs() < 0.0001 && (rows[6][1] - 1.0).abs() < 0.006,
        "normalized Rayleigh phase constant-environment oracle: {:?}",
        rows[6]
    );
    let normal_ggx =
        (1.0 - fresnel(1.0, 1.0, 1.333)) * 0.04 / (4.0 * std::f64::consts::PI * 0.9f64.powi(4));
    assert!(
        (rows[3][1] - normal_ggx).abs() < 0.0001,
        "normal near-smooth water connection times independent unchanged GGX response {normal_ggx}: {:?}",
        rows[3]
    );
    let expected = independent_ggx_energy();
    assert!(
        (rows[7][0] - expected).abs() < 0.0003 && (rows[7][1] - expected).abs() < 0.0005,
        "independent f64 GGX hemispherical quadrature energy {expected}: {:?}",
        rows[7]
    );
    let expected_phase = (1.0 - fresnel(1.0, 1.0, 1.333)) * 3.0 / (8.0 * std::f64::consts::PI);
    // At normal incidence, reciprocal radiance and Snell solid-angle factors
    // cancel; the narrow water connection retains transmitted directional flux.
    assert!(
        (rows[0][1] - expected_phase).abs() < 0.006,
        "normal reciprocal Snell radiance/phase oracle {expected_phase}: {:?}",
        rows[0]
    );
}
fn independent_ggx_energy() -> f64 {
    let roughness = 0.9f64;
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    let k = (roughness + 1.0).powi(2) / 8.0;
    let steps = 65536;
    let mut sum = 0.0;
    for index in 0..steps {
        let nl = (index as f64 + 0.5) / steps as f64;
        let nh = ((1.0 + nl) * 0.5).sqrt();
        let vh = nh;
        let distribution = a2 / (std::f64::consts::PI * (nh * nh * (a2 - 1.0) + 1.0).powi(2));
        let geometry = nl / (nl * (1.0 - k) + k);
        let f = 0.04 + 0.96 * (1.0 - vh).powi(5);
        sum += f * distribution * geometry / 4.0;
    }
    sum * 2.0 * std::f64::consts::PI / steps as f64
}

// Captured pre-guide GGX scatter contract: proposed samples must retain this
// per-direction BRDF/PDF ratio, including grazing masks and conductor Smith.
const GGX_REFERENCE: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let row=u32(pixel.x);let n=vec3f(0.0,1.0,0.0);
 let views=array<f32,3>(1.0,0.2,0.02);let nv=views[row%3u];let v=vec3f(sqrt(1.0-nv*nv),nv,0.0);
 let rough=select(0.9,0.3,(row/3u)%2u==1u);
 var pbr=bg_decode_pbr(vec4f(1.0-rough,0.0,0.0,0.0),vec3f(0.6,0.3,0.2),false,true);
 if row>=6u {pbr=bg_decode_pbr(vec4f(1.0-rough,231.0/255.0,0.0,0.0),vec3f(0.6,0.3,0.2),true,true);}
 let alpha=rough*rough;var state=1973u*row+1881u;var error=0.0;var energy=0.0;
 for(var i=0u;i<1024u;i++) {
  let xi=vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state));
  let cosine=sqrt((1.0-xi.x)/(1.0+(alpha*alpha-1.0)*xi.x));let sine=sqrt(max(0.0,1.0-cosine*cosine));let azimuth=2.0*RAY_PI*xi.y;
  let h=ray_basis(n)*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
  let d=reflect(-v,h);let nl=max(dot(n,d),0.0);let vh=max(dot(v,h),0.0);
  let k=(rough+1.0)*(rough+1.0)/8.0;
  var geometry=nl/max(nl*(1.0-k)+k,0.0001)*nv/max(nv*(1.0-k)+k,0.0001);
  if pbr.preset_id!=0u {geometry=1.0/(1.0+bg_ggx_lambda(nl,alpha)+bg_ggx_lambda(nv,alpha));}
  let expected=select(vec3f(0.0),bg_pbr_fresnel(vh,pbr)*geometry*vh/max(nv*cosine,0.0001),nl>0.0);
  let sample=ray_water_guided_ggx(n,v,pbr,RayWaterGuide(n,1.0,false),xi);
  let relative=abs(sample.weight-expected)/max(abs(expected),vec3f(0.0001));
  error=max(error,max(relative.x,max(relative.y,relative.z)));energy+=sample.weight.x;
 }
 return vec4f(error,energy/1024.0,nv,rough);
}
"#;
#[test]
fn gpu_guided_ggx_disabled_matches_original_grazing_dielectric_and_conductor_scatter() {
    let (device, queue) = device();
    let source = lobe_source().replace(LOBES, GGX_REFERENCE);
    let rows = draw(&device, &queue, &source, 12, 1, &[], &[]);
    for row in rows {
        assert!(
            row.iter().all(|v| v.is_finite()) && row[0] < 0.015 && row[1] > 0.0,
            "full GGX proposal preserves original direction-dependent energy at grazing angles and conductor Smith: {row:?}"
        );
    }
}
