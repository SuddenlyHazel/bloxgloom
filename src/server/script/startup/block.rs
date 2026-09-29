//! Bounded options for one-state package-owned blocks and crossed plants.
//! Keep the old three-argument declaration's block definition unchanged.
use crate::server::script::values::{integer, text};
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
    for (index, pair) in options.pairs::<Value, Value>().enumerate() {
        if index >= 11 {
            return Err("too many block options");
        }
        let (key, value) = pair.map_err(|_| "invalid block option")?;
        let Value::String(key) = key else {
            return Err("invalid block option key");
        };
        match key.as_bytes().as_ref() {
            b"flammable" => block.flammable = boolean(value)?,
            b"supports_plant" => block.supports_plant = boolean(value)?,
            b"solid" => block.solid = boolean(value)?,
            b"replaceable" => block.replaceable = boolean(value)?,
            b"side" => block.textures.side = text(value)?,
            b"bottom" => block.textures.bottom = text(value)?,
            b"emission" => block.emission = integer(value, 0, 15)? as u8,
            b"reflectance" => {
                let Value::Table(table) = value else {
                    return Err("reflectance must be a three-byte array");
                };
                if table.raw_len() != 3
                    || table.clone().pairs::<Value, Value>().take(4).count() != 3
                {
                    return Err("reflectance must have exactly three channels");
                }
                for (index, channel) in block.reflectance.iter_mut().enumerate() {
                    *channel = integer(
                        table
                            .raw_get(index + 1)
                            .map_err(|_| "invalid reflectance")?,
                        0,
                        255,
                    )? as u8;
                }
            }
            b"geometry" => {
                block.geometry = match text(value)?.as_str() {
                    "cube" => Geometry::Cube,
                    "crossed_plant" => Geometry::CrossedPlant,
                    "narrow_crossed_plant" => Geometry::NarrowCrossedPlant,
                    _ => return Err("unsupported block geometry"),
                };
            }
            b"material" => {
                block.material = match text(value)?.as_str() {
                    "opaque" => Material::Opaque,
                    "cutout" => Material::Cutout,
                    _ => return Err("unsupported block material"),
                };
            }
            _ => return Err("unknown block option"),
        }
    }
    if block.geometry != Geometry::Cube && (block.material != Material::Cutout || block.solid) {
        return Err("crossed plants require cutout material and solid=false");
    }
    Ok(block)
}

fn boolean(value: Value) -> Result<bool, &'static str> {
    match value {
        Value::Boolean(value) => Ok(value),
        _ => Err("block option must be boolean"),
    }
}

pub(in crate::server::script) fn extended(block: &Block) -> bool {
    block.textures.top != block.textures.side
        || block.textures.top != block.textures.bottom
        || !block.solid
        || block.replaceable
        || block.emission != 0
        || block.reflectance != [128; 3]
}

pub(in crate::server::script) fn visual(block: &Block) -> bool {
    block.geometry != Geometry::Cube || block.material != Material::Opaque
}
