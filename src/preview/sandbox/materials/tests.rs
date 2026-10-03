use super::*;

#[test]
fn steel_companions_are_linear_data_and_registered_before_catalog_freeze() {
    let mut catalog = Catalog::builtins();
    install_sandbox_materials(&mut catalog).unwrap();
    assert!(catalog.state_by_key("sandbox:steel").is_some());
    let specular = catalog
        .textures()
        .iter()
        .find(|texture| texture.key == "sandbox:steel_s")
        .unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(specular.png.as_ref()))
        .read_info()
        .unwrap();
    assert!(
        reader.info().srgb.is_none(),
        "companion channels are linear data"
    );
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut bytes).unwrap();
    assert_eq!(&bytes[..4], &[75, 0, 0, 255], "rough seam");
    assert_eq!(
        &bytes[(2 * 16 + 2) * 4..][..4],
        &[205, 255, 0, 255],
        "rivet"
    );
    assert_eq!(
        &bytes[(8 * 16 + 8) * 4..][..4],
        &[160, 255, 0, 255],
        "satin panel"
    );
    assert!(
        catalog
            .textures()
            .iter()
            .any(|texture| texture.key == "sandbox:steel_n")
    );
    assert!(
        catalog
            .textures()
            .iter()
            .all(|texture| texture.key != "sandbox:cyan_s")
    );
    catalog.validate().unwrap();
}
