//! Bounded options for one-state package-owned blocks and crossed plants.
//! Keep the old three-argument declaration's block definition unchanged.
use crate::server::script::values::{integer, text};
use bloxgloom_host_api::content::{Block, BlockState, FaceTextures, Geometry, Material};
use mlua::Value;
mod states;

pub(in crate::server::script) fn cube(
    key: String,
    name: String,
    texture: String,
    options: Value,
) -> Result<Block, &'static str> {
    let mut block = Block {
        acoustics: None,
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
        sky_attenuation: 0,
        reflectance: [128; 3],
        properties: vec![],
        states: vec![BlockState::default()],
    };
    let options = match options {
        Value::Nil => return Ok(block),
        Value::Table(table) => table,
        _ => return Err("block options must be a table"),
    };
    let mut properties = None;
    let mut states = None;
    for (index, pair) in options.pairs::<Value, Value>().enumerate() {
        if index >= 15 {
            return Err("too many block options");
        }
        let (key, value) = pair.map_err(|_| "invalid block option")?;
        let Value::String(key) = key else {
            return Err("invalid block option key");
        };
        match key.as_bytes().as_ref() {
            b"acoustics" => block.acoustics = Some(super::acoustics::decode(value)?),
            b"flammable" => block.flammable = boolean(value)?,
            b"supports_plant" => block.supports_plant = boolean(value)?,
            b"solid" => block.solid = boolean(value)?,
            b"replaceable" => block.replaceable = boolean(value)?,
            b"side" => block.textures.side = text(value)?,
            b"bottom" => block.textures.bottom = text(value)?,
            b"sky_attenuation" => block.sky_attenuation = integer(value, 0, 15)? as u8,
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
            b"properties" => properties = Some(value),
            b"states" => states = Some(value),
            _ => return Err("unknown block option"),
        }
    }
    if properties.is_some() || states.is_some() {
        block.properties = states::properties(properties.ok_or("states require properties")?)?;
        block.states = states::states(
            states.ok_or("properties require explicit states")?,
            &block.properties,
        )?;
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

pub(in crate::server::script) fn stateful(block: &Block) -> bool {
    !block.properties.is_empty()
        || block.states.len() != 1
        || block.states[0].emission.is_some()
        || block.states[0].textures.is_some()
}

pub(in crate::server::script) fn placement_state(block: &Block) -> String {
    let mut states = block
        .states
        .iter()
        .map(|state| {
            let mut properties = state.properties.clone();
            properties.sort();
            let suffix = properties
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(",");
            (properties, suffix)
        })
        .collect::<Vec<_>>();
    states.sort_by(|a, b| a.0.cmp(&b.0));
    if states[0].1.is_empty() {
        block.key.clone()
    } else {
        format!("{}[{}]", block.key, states[0].1)
    }
}

pub(in crate::server::script) fn has_state(block: &Block, key: &str) -> bool {
    block.states.iter().any(|state| {
        let mut properties = state.properties.clone();
        properties.sort();
        let suffix = properties
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        if suffix.is_empty() {
            key == block.key
        } else {
            key == format!("{}[{suffix}]", block.key)
        }
    })
}
