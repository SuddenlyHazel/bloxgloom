//! Complete production transport source, shared by validation and rendering.
#[cfg(test)]
pub(super) fn transport() -> String {
    transport_for_lobes(false)
}
#[cfg(test)]
pub(super) fn transport_for_lobes(enabled: bool) -> String {
    transport_for_modes(enabled, false)
}
pub(super) fn transport_for_modes(enabled: bool, raw_samples: bool) -> String {
    transport_with_lod_vector(enabled, raw_samples, super::lod::vector::configured())
}
pub(super) fn transport_with_lod_vector(enabled: bool, raw_samples: bool, vector: bool) -> String {
    let source = format!(
        "const RAY_MATERIAL_FAST:bool={};\nconst RAY_NEAR_FIRST:bool={};\nconst RAY_LOD_TIERED:bool={};\nconst RAY_SUN_SKIP:bool={};\nconst RAY_CAUSTIC_GUIDE:bool={};\nconst RAY_CAUSTIC_PHASE_GUIDE:bool={};\nconst RAY_CAUSTIC_GGX_GUIDE:bool={};\nconst RAY_WATER_COMPONENT:u32={}u;\n{}",
        super::super::optimizations::material_fast(),
        std::env::var("BLOXGLOOM_GI_NEAR_FIRST").as_deref() == Ok("1"),
        std::env::var("BLOXGLOOM_GI_LOD_TIERED").as_deref() != Ok("0"),
        std::env::var("BLOXGLOOM_GI_SUN_SKIP").as_deref() != Ok("0"),
        std::env::var("BLOXGLOOM_GI_CAUSTIC_GUIDE").as_deref() != Ok("0"),
        std::env::var("BLOXGLOOM_GI_PHASE_GUIDE").as_deref() != Ok("0"),
        std::env::var("BLOXGLOOM_GI_GGX_GUIDE").as_deref() != Ok("0"),
        std::env::var("BLOXGLOOM_GI_WATER_COMPONENT")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value <= 2)
            .unwrap_or(0),
        [
            crate::render::sky::environment_shader(),
            include_str!("../../material/pbr.wgsl").to_owned(),
            include_str!("../../material/foliage.wgsl").to_owned(),
            include_str!("../../material/foliage_optics.wgsl").to_owned(),
            include_str!("../../water/waves.wgsl").to_owned(),
            include_str!("../intersection.wgsl").to_owned(),
            include_str!("../coverage.wgsl").to_owned(),
            include_str!("../volume.wgsl").replace(
                "const RAY_KNOWN_CERTIFICATE:bool=false;",
                if std::env::var("BLOXGLOOM_GI_KNOWN_CERTIFICATE").as_deref() == Ok("1") {
                    "const RAY_KNOWN_CERTIFICATE:bool=true;"
                } else {
                    "const RAY_KNOWN_CERTIFICATE:bool=false;"
                }
            ),
            super::lod::vector::source_for(
                &super::lod::SHADER.replace(
                    "const RAY_LOD_ROOT_ORDER:bool=false;",
                    if std::env::var("BLOXGLOOM_GI_LOD_ROOT_ORDER").as_deref() == Ok("1") {
                        "const RAY_LOD_ROOT_ORDER:bool=true;"
                    } else {
                        "const RAY_LOD_ROOT_ORDER:bool=false;"
                    }
                ),
                vector
            ),
            super::super::dynamic::shader(crate::content::catalog()),
            include_str!("../water.wgsl").to_owned(),
            super::lobes::packet(enabled),
            include_str!("../water/lobes/sample.wgsl").to_owned(),
            include_str!("../water/caustics.wgsl").replace(
                "const RAY_GUIDE_BOUNDARY_FIRST:bool=true;",
                if std::env::var("BLOXGLOOM_GI_GUIDE_BOUNDARY_FIRST").as_deref() == Ok("0") {
                    "const RAY_GUIDE_BOUNDARY_FIRST:bool=false;"
                } else {
                    "const RAY_GUIDE_BOUNDARY_FIRST:bool=true;"
                },
            ),
            include_str!("../water/ggx.wgsl").to_owned(),
            include_str!("../water/diagnostics.wgsl").to_owned(),
            include_str!("../denoise.wgsl").to_owned(),
            include_str!("../transport.wgsl").to_owned(),
            include_str!("../medium.wgsl").to_owned(),
            include_str!("../paired.wgsl").to_owned()
        ]
        .join("\n")
    );
    super::lobes::raw_source(source, raw_samples)
}

pub(super) fn composition(water_lobes: bool, filtering: bool) -> String {
    super::lobes::filter_source(
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}",
            crate::render::sky::STYLE_SHADER,
            include_str!("../../material/sky_prefilter.wgsl"),
            include_str!("../../material/pbr.wgsl"),
            include_str!("../denoise.wgsl"),
            include_str!("../filter.wgsl"),
            include_str!("../composite.wgsl"),
            super::lobes::packet(water_lobes)
        ),
        filtering,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn complete_transport_shader_validates_all_target_and_medium_paths() {
        for (enabled, raw_samples) in [(false, false), (true, false), (true, true)] {
            let source = super::transport_for_modes(enabled, raw_samples);
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{error:?}"));
        }
    }
    #[test]
    fn first_water_raw_moments_and_optical_filter_modules_validate() {
        let mut sources = vec![include_str!("../water/lobes/moments.wgsl").to_owned()];
        for (split, filtering) in [(false, false), (true, false), (true, true)] {
            let source = super::composition(split, filtering);
            assert_eq!(source.contains("var ray_lobe_guide:"), filtering);
            sources.push(source);
        }
        let plain = super::transport_for_lobes(true);
        assert!(!plain.contains("texture_storage_2d_array"));
        assert_eq!(super::super::lobes::raw_source(plain.clone(), false), plain);
        for source in sources {
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{error:?}"));
        }
    }
}
