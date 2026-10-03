//! V52 carries opt-in foliage shading and voxel sky attenuation as frozen data.
//! Default packages retain their exact earlier grammar and bytes.
use super::*;
use content::FoliageShading;
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x34";

pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    let mut textures: Vec<_> = declarations
        .textures
        .iter()
        .filter(|t| t.definition.foliage != FoliageShading::default())
        .collect();
    let mut blocks: Vec<_> = declarations
        .blocks
        .iter()
        .filter(|b| b.sky_attenuation != 0)
        .collect();
    if textures.is_empty() && blocks.is_empty() {
        return Ok(bundle);
    }
    textures.sort_by(|a, b| a.definition.key.cmp(&b.definition.key));
    blocks.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(textures.len())?;
    for texture in textures {
        let shading = texture.definition.foliage;
        if !shading.valid() {
            return Err(invalid());
        }
        writer.field(texture.definition.key.as_bytes())?;
        writer.field(
            &[
                shading.wrap.to_le_bytes(),
                shading.transmission.to_le_bytes(),
            ]
            .concat(),
        )?;
    }
    writer.count(blocks.len())?;
    for block in blocks {
        if block.sky_attenuation > 15 {
            return Err(invalid());
        }
        writer.field(block.key.as_bytes())?;
        writer.field(&[block.sky_attenuation])?;
    }
    let key = CacheKey(Sha256::digest(&writer.0).into());
    ClientBundle::decode_verify(&writer.0, key)
}

pub(in crate::server::script::package::client) fn decode(
    bytes: &[u8],
    expected: CacheKey,
) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.get(..8) == Some(b"BGCLIENT")
        && inner.get(8).is_some_and(|version| *version >= MAGIC[8])
    {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let textures = reader.count(startup.textures.len())?;
    let mut previous = String::new();
    for _ in 0..textures {
        let key = reader.text(128)?;
        if key <= previous {
            return Err(invalid());
        }
        previous.clone_from(&key);
        let texture = startup
            .textures
            .iter_mut()
            .find(|t| t.key == key)
            .ok_or_else(invalid)?;
        if texture.foliage != FoliageShading::default() {
            return Err(invalid());
        }
        texture.foliage = shading(reader.field(8)?)?;
    }
    let blocks = reader.count(startup.blocks.len())?;
    previous.clear();
    for _ in 0..blocks {
        let key = reader.text(128)?;
        if key <= previous {
            return Err(invalid());
        }
        previous.clone_from(&key);
        let block = startup
            .blocks
            .iter_mut()
            .find(|b| b.key == key)
            .ok_or_else(invalid)?;
        if block.sky_attenuation != 0 {
            return Err(invalid());
        }
        block.sky_attenuation = match reader.field(1)? {
            [value @ 1..=15] => *value,
            _ => return Err(invalid()),
        };
    }
    if textures + blocks == 0 || !reader.0.is_empty() {
        return Err(invalid());
    }
    bundle.residency.resize(bytes.len())?;
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}

fn shading(bytes: &[u8]) -> Result<FoliageShading, ScriptError> {
    if bytes.len() != 8 {
        return Err(invalid());
    }
    let result = FoliageShading {
        wrap: f32::from_le_bytes(bytes[..4].try_into().unwrap()),
        transmission: f32::from_le_bytes(bytes[4..].try_into().unwrap()),
    };
    if !result.valid() || result == FoliageShading::default() {
        return Err(invalid());
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
