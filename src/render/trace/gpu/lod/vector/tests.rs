use super::*;
use crate::render::trace::scene::{Node, Scene, Triangle, pages};

#[test]
fn scalar_mode_is_byte_identical_and_vector_pages_keep_record_alignment() {
    let scalar = include_str!("../../../lod.wgsl");
    assert_eq!(source_for(scalar, false).as_bytes(), scalar.as_bytes());
    assert_eq!(std::mem::size_of::<Node>(), 48);
    assert_eq!(std::mem::size_of::<Triangle>(), 96);
    let scene = Scene::build([]);
    assert_eq!(pages::packed_words(&scene), vec![0, 0, 4, 4]);
    let vector = source_for(scalar, true);
    assert_eq!(vector.matches("var<storage,read> ray_lod_page_").count(), 4);
    assert_eq!(vector.matches("array<vec4u>").count(), 4);
    assert!(!vector.contains("array<u32>"));
    let ordered = scalar.replace(
        "const RAY_LOD_ROOT_ORDER:bool=false;",
        "const RAY_LOD_ROOT_ORDER:bool=true;",
    );
    assert_eq!(
        source_for(&ordered, true).as_bytes(),
        ordered.as_bytes(),
        "unsupported native combined mode must use exact scalar bytes"
    );
}

#[test]
fn vector_complete_transport_validates_without_extra_bindings_or_modes() {
    for (split, raw) in [(false, false), (true, false), (true, true)] {
        let scalar =
            crate::render::trace::gpu::shaders::transport_with_lod_vector(split, raw, false);
        let vector = source_for(&scalar, true);
        let module = wgpu::naga::front::wgsl::parse_str(&vector)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&vector)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
