mod storage;

#[test]
fn water_lobe_packet_respects_default_and_portable_mrt_budget() {
    use wgpu::TextureFormat::{R16Float, Rgba16Float};
    assert_eq!(super::format(false), R16Float);
    assert_eq!(super::format(true), Rgba16Float);
    assert_eq!(
        3 * 8 + 8,
        wgpu::Limits::default().max_color_attachment_bytes_per_sample
    );
    assert!(super::packet(false).contains("const RAY_WATER_LOBES:bool=false;"));
    assert!(super::packet(true).contains("const RAY_WATER_LOBES:bool=true;"));
}
