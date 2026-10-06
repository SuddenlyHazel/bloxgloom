//! Opt-in first-water lobe bookkeeping. Default targets/source stay unchanged.
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
