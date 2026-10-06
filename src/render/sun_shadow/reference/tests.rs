use super::*;
#[test]
fn source_shadow_settings_are_bounded_and_off_stays_off() {
    let standard = Settings::for_quality(crate::config::SunShadowQuality::Medium, 4096);
    let reference = configure(standard, true, 4096);
    assert_eq!(
        (reference.resolution, reference.distance, reference.filter),
        (2048, 256.0, 1.0)
    );
    assert_eq!(data(reference), [1.0, 256.0, 0.9, 0.0]);
    assert_eq!(configure(standard, false, 4096).distance, standard.distance);
    assert!(
        !configure(
            Settings::for_quality(crate::config::SunShadowQuality::Off, 4096),
            true,
            4096
        )
        .reference
    );
    assert_eq!(configure(standard, true, 256).distance, 0.0);
}

mod gpu;

#[test]
fn source_shadow_bound_helpers_validate_before_gpu() {
    let source = format!(
        "{}\n{}\n{}",
        super::super::SHADER,
        super::shader(),
        r#"
 struct V { @builtin(position) clip:vec4f,@location(0) world:vec3f };
 @vertex fn vs_main(@builtin(vertex_index) i:u32)->V {
  let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));
  let world=vec3f(xy*2.0-1.0,0.0);return V(bg_shadow_project(world),world);
 }
 @fragment fn fs_main(v:V)->@location(0) vec4f {
  let receiver=bg_shadow_receiver(v.world);
  return vec4f(bg_sun_visibility_material(receiver,vec3f(0.0,1.0,0.0),1.0,0.0));
 }
 "#
    );
    let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
