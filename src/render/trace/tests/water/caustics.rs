//! Guided sampling changes the proposal, never the water/diffuse response.
use super::*;
const CAUSTIC: &str = r#"
fn caustic_response(direction:vec3f,sun:vec3f)->f32 {
 let boundary=RayWaterInterface(vec3f(0.0,-1.0,0.0),vec3f(0.0,1.0,0.0),0.12,RAY_WATER_IOR,1.0);
 let evaluation=ray_water_eval(boundary,direction,sun);
 return evaluation.f.x*sun.y;
}
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let row=u32(pixel.y);let n=vec3f(0.0,1.0,0.0);
 let heights=array<f32,3>(1.0,0.5,0.1);let h=heights[row%3u];
 let sun=vec3f(sqrt(1.0-h*h),h,0.0);
 let guide=RayWaterGuide(normalize(-refract(-sun,n,1.0/RAY_WATER_IOR)),cos(ray_water_guide_angle(0.12,h)),true);
 let disabled=RayWaterGuide(n,1.0,false);
 var state=9881u+row*991u+u32(pixel.x)*1973u;var old_mean=0.0;var new_mean=0.0;var old_second=0.0;var new_second=0.0;
 for(var i=0u;i<512u;i++) {
  let old=ray_water_guide_sample(n,disabled,vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state)));
  let proposed=ray_water_guide_sample(n,guide,vec3f(random_fixture(&state),random_fixture(&state),random_fixture(&state)));
  var old_value=caustic_response(old.direction,sun);var new_value=proposed.weight*caustic_response(proposed.direction,sun);
  if row==3u {old_value=1.0;new_value=proposed.weight;}
  let old_delta=old_value-old_mean;old_mean+=old_delta/f32(i+1u);old_second+=old_delta*(old_value-old_mean);
  let new_delta=new_value-new_mean;new_mean+=new_delta/f32(i+1u);new_second+=new_delta*(new_value-new_mean);
 }
 return vec4f(old_mean,new_mean,old_second/512.0+old_mean*old_mean,new_second/512.0+new_mean*new_mean);
}
"#;
fn caustic_source() -> String {
    let rng = FIXTURE.split("@fragment fn fs_main").next().unwrap();
    let helpers = include_str!("../../water/caustics.wgsl")
        .split("// Querying")
        .next()
        .unwrap();
    source().replace(FIXTURE, &format!("{rng}\n{helpers}\n{CAUSTIC}"))
}
#[test]
fn gpu_water_caustic_guide_keeps_diffuse_energy_and_reduces_solar_variance() {
    let (device, queue) = device();
    let pixels = draw(&device, &queue, &caustic_source(), 1024, 4, &[], &[]);
    let mut rows = [[0.0f64; 4]; 4];
    for (row, line) in pixels.chunks_exact(1024).enumerate() {
        for pixel in line {
            for (sum, value) in rows[row].iter_mut().zip(pixel) {
                *sum += f64::from(*value) / 1024.0;
            }
        }
        rows[row][2] -= rows[row][0] * rows[row][0];
        rows[row][3] -= rows[row][1] * rows[row][1];
    }
    for (angle, row) in rows[..3].iter().enumerate() {
        let error = (row[0] - row[1]).abs();
        let standard_error = ((row[2] + row[3]) / 524288.0).sqrt();
        println!(
            "caustic solar cosine={} old_mean={} new_mean={} old_variance={} new_variance={} stderr={}",
            [1.0, 0.5, 0.1][angle],
            row[0],
            row[1],
            row[2],
            row[3],
            standard_error
        );
        assert!(row.iter().all(|v| v.is_finite()) && row[0] > 0.0 && row[1] > 0.0);
        assert!(
            error < 5.0 * standard_error + 0.0005,
            "same water-BRDF energy under old/new sampling: {row:?}"
        );
        assert!(
            row[3] < row[2] * 0.15,
            "solar-cone proposal should materially reduce caustic variance: {row:?}"
        );
    }
    let normal_expected = (1.0 - fresnel(1.0, 1.0, 1.333)) / std::f64::consts::PI;
    assert!(
        (rows[0][1] - normal_expected).abs() < 0.015,
        "flat near-smooth normal-incidence caustic agrees with transmitted directional irradiance: {:?}",
        rows[0]
    );
    assert!(
        (rows[3][0] - 1.0).abs() < 0.0001 && (rows[3][1] - 1.0).abs() < 0.012,
        "retained cosine support preserves a constant environment: {:?}",
        rows[3]
    );
}
