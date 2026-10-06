//! Analytic absorption stays deterministic while physical scattering remains sampled.
use super::*;

const ABSORPTION: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let row=u32(pixel.x);let index=row%4u;
 let lengths=array<f32,4>(3.0,12.0,30.0,100.0);let limit=lengths[index];
 var state=1907u+index*19937u;var survival=vec3f(0.0);var collision=vec3f(0.0);
 var survived=0u;
 var old_mean=0.0;var old_second=0.0;var new_mean=0.0;var new_second=0.0;
 let old_probability=dot(exp(-RAY_WATER_EXTINCTION*limit),vec3f(1.0/3.0));
 for(var i=0u;i<131072u;i++) {
  let sample=ray_water_medium_sample(limit,vec2f(random_fixture(&state),random_fixture(&state)));
  var blue=0.0;
  if sample.event {collision+=sample.weight;} else {survival+=sample.weight;blue=sample.weight.b;survived++;}
  let new_delta=blue-new_mean;new_mean+=new_delta/f32(i+1u);new_second+=new_delta*(blue-new_mean);
  let channel=min(u32(random_fixture(&state)*3.0),2u);
  let old_distance=-log(max(1.0-random_fixture(&state),0.00000006))/RAY_WATER_EXTINCTION[channel];
  let old=select(0.0,exp(-RAY_WATER_EXTINCTION.b*limit)/old_probability,old_distance>=limit);
  let old_delta=old-old_mean;old_mean+=old_delta/f32(i+1u);old_second+=old_delta*(old-old_mean);
 }
 if row<4u {return vec4f(survival/131072.0,f32(survived)/131072.0);}
 if row<8u {return vec4f(collision/131072.0,1.0);}
 return vec4f(old_second/131072.0,new_second/131072.0,old_mean,new_mean);
}
"#;
fn absorption_source() -> String {
    // Preserve the independent fixture RNG, while replacing only its entrypoint.
    let rng = FIXTURE.split("@fragment fn fs_main").next().unwrap();
    source().replace(FIXTURE, &format!("{rng}\n{ABSORPTION}"))
}
#[test]
fn gpu_water_scattering_only_free_flight_preserves_energy_and_reduces_absorption_variance() {
    let (device, queue) = device();
    let rows = draw(&device, &queue, &absorption_source(), 12, 1, &[], &[]);
    let scattering = [0.0006803136f64, 0.0014, 0.0033313234];
    let extinction = [0.3406803136f64, 0.0579, 0.0125513234];
    for (index, distance) in [3.0, 12.0, 30.0, 100.0].into_iter().enumerate() {
        for (channel, sigma) in extinction.iter().copied().enumerate() {
            let beer = (-sigma * distance).exp();
            assert!(
                (f64::from(rows[index][channel]) - beer).abs() < 0.012,
                "Beer survival unchanged at {distance}m: {:?}",
                rows[index]
            );
            let scatter = scattering[channel] / sigma * (1.0 - beer);
            assert!(
                (f64::from(rows[index + 4][channel]) - scatter).abs() < 0.002,
                "physical collision energy unchanged at {distance}m: {:?}",
                rows[index + 4]
            );
        }
        let p_old = extinction
            .iter()
            .map(|s| (-s * distance).exp())
            .sum::<f64>()
            / 3.0;
        let p_new = scattering
            .iter()
            .map(|s| (-s * distance).exp())
            .sum::<f64>()
            / 3.0;
        assert!(
            (f64::from(rows[index][3]) - p_new).abs() < 0.003,
            "unweighted free-flight survival uses the scattering-only mixture CDF: {:?}",
            rows[index]
        );
        let blue = (-extinction[2] * distance).exp();
        let old_variance = blue * blue * (1.0 / p_old - 1.0);
        let new_variance = blue * blue * (1.0 / p_new - 1.0);
        let actual = rows[index + 8];
        assert!(
            (f64::from(actual[0]) / old_variance - 1.0).abs() < 0.03,
            "old extinction roulette variance oracle: {actual:?}"
        );
        assert!(
            (f64::from(actual[1]) / new_variance - 1.0).abs() < 0.10,
            "new scattering-only variance oracle: {actual:?}"
        );
        assert!(
            actual[1] < actual[0] * 0.06,
            "absorption sampling variance should decrease without a throughput clamp: {actual:?}"
        );
        assert!(
            (f64::from(actual[2]) - blue).abs() < 0.012
                && (f64::from(actual[3]) - blue).abs() < 0.012,
            "both estimators retain identical mean energy: {actual:?}"
        );
    }
}
