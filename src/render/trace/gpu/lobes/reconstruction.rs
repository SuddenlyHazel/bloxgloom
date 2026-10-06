//! Optional raw sample/confidence resources, separate from the transport estimator.
mod capabilities;
mod encode;
mod resources;

pub(in crate::render::trace) fn requested() -> bool {
    capabilities::requested()
}
pub(in crate::render::trace) fn supported(adapter: &wgpu::Adapter) -> bool {
    capabilities::supported(adapter)
}

pub(in crate::render::trace::gpu) struct Reconstruction {
    pub(in crate::render::trace::gpu) raw: wgpu::TextureView,
    pub(in crate::render::trace::gpu) moments: [wgpu::TextureView; 2],
    pub(in crate::render::trace::gpu) guide: [wgpu::TextureView; 2],
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    pub(in crate::render::trace::gpu) filtering: bool,
}

pub(in crate::render::trace::gpu) fn raw_source(source: String, enabled: bool) -> String {
    if !enabled {
        return source;
    }
    let additions = [
        (
            "    var max_age=select(32.0,6.0,raster_water&&!complete_path);",
            concat!(
                "    let raw_delta=delta;let raw_reflection=reflection;var raw_hp=vec2i(-1);\n",
                "    var max_age=select(32.0,6.0,raster_water&&!complete_path);"
            ),
        ),
        (
            "        if compatible {\n",
            "        if compatible {\n            raw_hp=hp;\n",
        ),
        (
            "    return RayTransportOutput(vec4f(delta,receiver.w),",
            concat!(
                "    let raw_eligible=complete_path&&first_water&&!eye_water&&!primary_actor\n",
                "        &&ray_frame.water.z<=0.5&&abs(first.normal.y)>0.999&&RAY_WATER_COMPONENT==0u;\n",
                "    ray_store_water_lobes(vec2i(frag.xy),raw_delta,raw_reflection,primary_t,n,receiver.z,raw_eligible,raw_hp);\n",
                "    return RayTransportOutput(vec4f(delta,receiver.w),"
            ),
        ),
    ];
    let mut source = source;
    for (original, replacement) in additions {
        assert_eq!(
            source.matches(original).count(),
            1,
            "raw lobe capture anchor changed"
        );
        source = source.replace(original, replacement);
    }
    source.push_str(include_str!("../../water/lobes/raw.wgsl"));
    source
}
