//! V46 transports frozen block acoustic metadata without server execution.
use super::*;
use content::{Acoustics, Habitat, ImpactProfile, RainSurface};
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x2e";
pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    let mut blocks = declarations
        .blocks
        .iter()
        .filter(|b| b.acoustics.is_some())
        .collect::<Vec<_>>();
    if blocks.is_empty() {
        return Ok(bundle);
    }
    blocks.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(blocks.len())?;
    for block in blocks {
        writer.field(block.key.as_bytes())?;
        writer.field(&block.acoustics.unwrap().bytes())?;
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
    // Compatibility wrappers descend strictly in version, bounding recursion.
    if inner.get(..8) == Some(b"BGCLIENT")
        && inner.get(8).is_some_and(|version| *version >= MAGIC[8])
    {
        return Err(invalid());
    }
    if inner.starts_with(MAGIC) {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(startup.blocks.len())?;
    if count == 0 {
        return Err(invalid());
    }
    let mut previous = String::new();
    for _ in 0..count {
        let key = reader.text(128)?;
        if key <= previous {
            return Err(invalid());
        }
        previous = key.clone();
        let block = startup
            .blocks
            .iter_mut()
            .find(|b| b.key == key)
            .ok_or_else(invalid)?;
        if block.acoustics.is_some() {
            return Err(invalid());
        }
        block.acoustics = Some(profile(reader.field(35)?)?);
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    bundle.residency.resize(bytes.len())?;
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
fn profile(bytes: &[u8]) -> Result<Acoustics, ScriptError> {
    if bytes.len() != 3 && bytes.len() != 35 {
        return Err(invalid());
    }
    let surface = match bytes[0] {
        0 => RainSurface::Water,
        1 => RainSurface::Dirt,
        2 => RainSurface::Leaf,
        3 => RainSurface::Concrete,
        4 => RainSurface::Glass,
        5 => RainSurface::Metal,
        6 => RainSurface::Plastic,
        7 => RainSurface::Asphalt,
        8 => RainSurface::AsphaltRoof,
        9 => RainSurface::Wood,
        _ => return Err(invalid()),
    };
    let habitat = match bytes[1] {
        0 => Habitat::None,
        1 => Habitat::Ground,
        2 => Habitat::Canopy,
        _ => return Err(invalid()),
    };
    let impact = match (bytes[2], bytes.len()) {
        (0, 3) => None,
        (1, 35) => {
            let v: Vec<_> = bytes[3..]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            Some(ImpactProfile {
                gain: v[0],
                click: v[1],
                frequency_hz: [v[2], v[3]],
                damping_per_s: [v[4], v[5]],
                resonance: v[6],
                lowpass_hz: v[7],
            })
        }
        _ => return Err(invalid()),
    };
    let acoustics = Acoustics {
        surface,
        habitat,
        impact,
    };
    if !acoustics.valid() {
        return Err(invalid());
    }
    Ok(acoustics)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acoustic_codec_rejects_nonfinite_unknown_and_noncanonical_payloads() {
        let acoustic = Acoustics {
            surface: RainSurface::Wood,
            habitat: Habitat::Canopy,
            impact: Some(ImpactProfile {
                gain: 1.,
                click: 0.5,
                frequency_hz: [450., 1100.],
                damping_per_s: [180., 350.],
                resonance: 0.65,
                lowpass_hz: 5000.,
            }),
        };
        assert_eq!(profile(&acoustic.bytes()).unwrap(), acoustic);
        let mut bad = acoustic.bytes();
        bad[3..7].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(profile(&bad).is_err());
        bad = acoustic.bytes();
        bad[1] = 3;
        assert!(profile(&bad).is_err());
        bad = acoustic.bytes();
        bad[2] = 0;
        assert!(profile(&bad).is_err());
        assert!(profile(&[10, 0, 0]).is_err());
        assert!(profile(&[0, 0, 0, 0]).is_err());
    }
}
