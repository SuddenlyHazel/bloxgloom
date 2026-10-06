//! Opt-in first-water lobe bookkeeping. Default targets/source stay unchanged.
mod diagnostics;
mod reconstruction;
pub(super) use reconstruction::Reconstruction;

pub(super) struct Mode {
    pub split: bool,
    pub reconstruction: bool,
    pub filtering: bool,
}
impl Mode {
    pub(super) fn configured(supported: bool) -> Self {
        let requested = reconstruction::requested();
        Self {
            split: configured() || requested,
            reconstruction: requested && supported,
            filtering: std::env::var("BLOXGLOOM_GI_WATER_FILTER").as_deref() != Ok("0"),
        }
    }
}
pub(in crate::render::trace) fn reconstruction_supported(adapter: &wgpu::Adapter) -> bool {
    reconstruction::supported(adapter)
}
pub(in crate::render::trace) fn reconstruction_requested() -> bool {
    reconstruction::requested()
}
pub(super) fn raw_source(source: String, enabled: bool) -> String {
    reconstruction::raw_source(source, enabled)
}
pub(super) fn filter_source(source: String, enabled: bool) -> String {
    if !enabled {
        return source;
    }
    let anchor = "    if ray_primary_actor(center_geometry) {return center;}";
    assert_eq!(
        source.matches(anchor).count(),
        1,
        "opted lobe filter anchor changed"
    );
    let mut source = source.replace(anchor, &format!(
        "{anchor}\n    if textureLoad(ray_lobe_guide,p,0).w>0.0 {{return ray_filter_first_water(p,center,center_geometry);}}"));
    source.push_str(include_str!("../water/lobes/filter.wgsl"));
    source
}
pub(super) fn configured() -> bool {
    std::env::var("BLOXGLOOM_GI_WATER_LOBES").as_deref() == Ok("1")
}
pub(super) fn format(enabled: bool) -> wgpu::TextureFormat {
    if enabled {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::R16Float
    }
}
pub(super) fn packet(enabled: bool) -> String {
    include_str!("../water/lobes/packet.wgsl").replace(
        "const RAY_WATER_LOBES:bool=false;",
        &format!("const RAY_WATER_LOBES:bool={enabled};"),
    )
}
#[cfg(test)]
mod tests;
