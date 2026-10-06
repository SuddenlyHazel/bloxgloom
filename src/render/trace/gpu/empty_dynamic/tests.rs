use super::*;

#[test]
fn disabled_empty_source_is_identical_and_all_target_modes_validate() {
    assert!(!allowed(false, false));
    assert!(!allowed(false, true));
    assert!(
        !allowed(true, true),
        "split/raw modes must retain the generic pipeline"
    );
    assert!(allowed(true, false));
    for (lobes, raw) in [(false, false), (true, false), (true, true)] {
        let source = super::super::shaders::transport_with_lod_vector(lobes, raw, false);
        assert_eq!(source_for(&source, false), source);
        let empty = source_for(&source, true);
        assert!(empty.contains("let primary_actor=false;"));
        assert!(!empty.contains("return dynamic_ray_cast_impl(origin,direction,limit,current,"));
        assert!(!empty.contains("{return dynamic_ray_surface(hit);}"));
        assert_eq!(empty.matches("if false {").count(), 2);
        let module = wgpu::naga::front::wgsl::parse_str(&empty)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&empty)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
    }
}
