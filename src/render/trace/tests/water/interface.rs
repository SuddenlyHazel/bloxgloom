//! Production wave/interface diagnostics at coast coordinates and grazing angles.
use super::*;
const PROBE: &str = r#"
@fragment fn fs_main(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let column=u32(pixel.x);let row=u32(pixel.y);
 let cosines=array<f32,4>(0.6,0.2,0.05,0.01);
 let clocks=array<f32,6>(0.0,0.5,2.0,8.0,20.0,40.0);
 let cosine=cosines[row%4u];ray_frame.water.x=clocks[row/4u];
 ray_triangles[0].normal=vec4f(0.0,1.0,0.0,0.0);
 let azimuth=f32(column%8u)*2.0*RAY_PI/8.0;
 let incoming=vec3f(sqrt(1.0-cosine*cosine)*cos(azimuth),-cosine,sqrt(1.0-cosine*cosine)*sin(azimuth));
 let position=vec3f(-656.0-f32(column/8u)*12.0,17.0,-2048.0-f32(column%8u)*12.0);
 let scale=0.1/cosine;
 let boundary=ray_water_interface(RayHit(0.0,0u,vec2f(0.0),vec3f(0.0,1.0,0.0)),incoming,position,vec4f(0.1,0.0,0.0,scale));
 var state=column*1973u+row*911u+139u;var reflected=0.0;var transmitted=0.0;var energy=0.0;
 for(var i=0u;i<128u;i++) {
  let r=ray_water_conditional_sample(boundary,incoming,vec2f(random_fixture(&state),random_fixture(&state)),false);
  let t=ray_water_conditional_sample(boundary,incoming,vec2f(random_fixture(&state),random_fixture(&state)),true);
  reflected+=select(0.0,1.0,any(r.weight!=vec3f(0.0)));
  transmitted+=select(0.0,1.0,any(t.weight!=vec3f(0.0)));
  energy+=r.weight.x+t.weight.x;
 }
 return vec4f(dot(boundary.normal,-incoming),reflected/128.0,transmitted/128.0,energy/128.0);
}
"#;
fn source_probe() -> String {
    let declarations = include_str!("../../intersection.wgsl")
        .split("@group(0)")
        .next()
        .unwrap();
    let rng = FIXTURE.split("@fragment fn fs_main").next().unwrap();
    format!(
        "{PRELUDE}\n{declarations}\nstruct WaterFrame{{water:vec4f}};var<private> ray_frame:WaterFrame;var<private> ray_triangles:array<RayTriangle,1>;fn ray_triangle_at(index:u32)->RayTriangle{{return ray_triangles[index];}}\n{}\n{}\n{rng}\n{PROBE}",
        include_str!("../../../water/waves.wgsl"),
        include_str!("../../water.wgsl")
    )
}
#[test]
fn gpu_production_water_interface_grazing_coast_wave_clock_and_hemisphere_probe() {
    let (device, queue) = device();
    let pixels = draw(&device, &queue, &source_probe(), 64, 24, &[], &[]);
    for row in 0..24 {
        let line = &pixels[row * 64..(row + 1) * 64];
        let mean = |channel: usize| line.iter().map(|p| p[channel]).sum::<f32>() / 64.0;
        let minimum = |channel: usize| {
            line.iter()
                .map(|p| p[channel])
                .fold(f32::INFINITY, f32::min)
        };
        println!(
            "water coast interface cosine={} time={} min_nv={} mean_R_valid={} mean_T_valid={} min_T_valid={} mean_energy={}",
            [0.6, 0.2, 0.05, 0.01][row % 4],
            [0.0, 0.5, 2.0, 8.0, 20.0, 40.0][row / 4],
            minimum(0),
            mean(1),
            mean(2),
            minimum(2),
            mean(3)
        );
        assert!(
            line.iter()
                .all(|p| p.iter().all(|v| v.is_finite()) && p[0] > 0.0 && p[2] > 0.99),
            "production interface must face the view and retain an air-to-water transmission branch: {line:?}"
        );
    }
}
