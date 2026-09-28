//! Bounded optional gameplay flags for one-state package-owned opaque cubes.
//! Keep the old three-argument declaration's block definition unchanged.
use bloxgloom_host_api::content::{Block, BlockState, FaceTextures, Geometry, Material};
use mlua::Value;

pub(in crate::server::script) fn cube(
    key: String,
    name: String,
    texture: String,
    options: Value,
) -> Result<Block, &'static str> {
    let mut block = Block {
        key,
        name,
        swatch: [1.0; 4],
        textures: FaceTextures::uniform(texture),
        geometry: Geometry::Cube,
        material: Material::Opaque,
        solid: true,
        replaceable: false,
        supports_plant: false,
        flammable: false,
        emission: 0,
        reflectance: [128; 3],
        properties: vec![],
        states: vec![BlockState::default()],
    };
    let options = match options {
        Value::Nil => return Ok(block),
        Value::Table(table) => table,
        _ => return Err("block options must be a table"),
    };
    for pair in options.pairs::<Value, Value>().take(3) {
        let (key, value) = pair.map_err(|_| "invalid block option")?;
        let Value::String(key) = key else {
            return Err("invalid block option key");
        };
        let Value::Boolean(value) = value else {
            return Err("block option must be boolean");
        };
        match key.as_bytes().as_ref() {
            b"flammable" => block.flammable = value,
            b"supports_plant" => block.supports_plant = value,
            _ => return Err("unknown block option"),
        }
    }
    Ok(block)
}
