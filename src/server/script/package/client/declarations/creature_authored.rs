//! V51 carries authored creature bindings; private state and scripts stay server-side.
use super::*;
use bloxgloom_host_api::entity::{Animation, AuthoredModel, Body, MAX_VISUAL_BYTES, MobileEntity};
use std::sync::Arc;
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x33";
pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    let mut creatures = declarations
        .creatures
        .iter()
        .filter(|c| c.authored_model.is_some())
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return Ok(bundle);
    }
    creatures.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(creatures.len())?;
    for c in creatures {
        c.validate().map_err(|_| invalid())?;
        let authored = c.authored_model.as_ref().unwrap();
        writer.field(c.key.as_bytes())?;
        writer.field(&c.schema_version.to_le_bytes())?;
        writer.field(&c.schema_fingerprint.to_le_bytes())?;
        writer.field(&((c.max_state_bytes - 12 - MAX_VISUAL_BYTES) as u16).to_le_bytes())?;
        writer.field(&c.interval.to_le_bytes())?;
        writer.field(&[
            c.read_radius,
            u8::from(c.reads_neighbours),
            u8::from(c.wakes_on_terrain_change),
        ])?;
        for f in [
            c.body.half_width,
            c.body.height,
            c.body.speed,
            c.animation.stride_rate,
            c.animation.stride_amplitude,
            c.animation.idle_rate,
            c.animation.idle_bob,
            c.animation.walk_bob,
            c.animation.fall_stretch,
            c.animation.landing_squash,
        ] {
            writer.field(&f.to_le_bytes())?;
        }
        writer.field(&c.interaction)?;
        writer.field(authored.key.as_bytes())?;
        writer.field(&authored.scale.to_le_bytes())?;
        for clip in [&authored.idle, &authored.walk, &authored.run] {
            writer.field(clip.as_deref().unwrap_or("").as_bytes())?;
        }
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
    if inner.get(..8) == Some(b"BGCLIENT") && inner.get(8).is_some_and(|v| *v >= MAGIC[8]) {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(MAX_PACKAGES * 8)?;
    if count == 0 {
        return Err(invalid());
    }
    let mut previous = String::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for c in &startup.creatures {
        *counts
            .entry(c.key.split_once(':').ok_or_else(invalid)?.0.into())
            .or_default() += 1;
    }
    for _ in 0..count {
        let key = reader.text(129)?;
        let (owner, local) = key.split_once(':').ok_or_else(invalid)?;
        if key <= previous
            || !identifier(local)
            || startup.creatures.iter().any(|c| c.key == key)
            || !startup.packages.iter().any(|p| {
                p.key == format!("{owner}:package")
                    && [composition::CONTENT, composition::MOBILE_ENTITIES]
                        .into_iter()
                        .all(|cap| p.requires.iter().any(|r| r == cap))
            })
        {
            return Err(invalid());
        }
        previous = key.clone();
        let n = counts.entry(owner.into()).or_default();
        *n += 1;
        if *n > 8 {
            return Err(invalid());
        }
        let schema_version =
            u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
        let schema_fingerprint =
            u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        let max_private =
            u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?) as usize;
        if !(1..=256).contains(&max_private) {
            return Err(invalid());
        }
        let interval = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
        if !(1..=1000).contains(&interval) {
            return Err(invalid());
        }
        let flags = reader.field(3)?;
        if flags.len() != 3 || flags[0] > 1 || flags[1] > 1 || flags[2] > 1 {
            return Err(invalid());
        }
        let (read_radius, reads_neighbours, wakes_on_terrain_change) =
            (flags[0], flags[1] != 0, flags[2] != 0);
        let body = Body {
            half_width: float(&mut reader)?,
            height: float(&mut reader)?,
            speed: float(&mut reader)?,
        };
        let animation = Animation {
            stride_rate: float(&mut reader)?,
            stride_amplitude: float(&mut reader)?,
            idle_rate: float(&mut reader)?,
            idle_bob: float(&mut reader)?,
            walk_bob: float(&mut reader)?,
            fall_stretch: float(&mut reader)?,
            landing_squash: float(&mut reader)?,
        };
        let interaction = reader.field(128)?.to_vec();
        let model_key = reader.text(129)?;
        if model_key.split_once(':').map(|p| p.0) != Some(owner) {
            return Err(invalid());
        }
        let scale = float(&mut reader)?;
        let asset = startup
            .models
            .iter()
            .find(|m| m.definition.key == model_key)
            .ok_or_else(invalid)?;
        let schema = asset.prepared.schema();
        let mut clip = || -> Result<Option<String>, ScriptError> {
            let raw = reader.field(96)?;
            if raw.is_empty() {
                return Ok(None);
            }
            let s = std::str::from_utf8(raw).map_err(|_| invalid())?.to_owned();
            if !schema.clips.contains(&s) {
                return Err(invalid());
            }
            Ok(Some(s))
        };
        let authored_model = Some(AuthoredModel {
            key: model_key,
            scale,
            idle: clip()?,
            walk: clip()?,
            run: clip()?,
        });
        let creature = MobileEntity {
            key,
            schema_version,
            schema_fingerprint,
            max_state_bytes: 12 + max_private + MAX_VISUAL_BYTES,
            max_public_bytes: 5 + MAX_VISUAL_BYTES,
            body,
            interval,
            read_radius,
            reads_neighbours,
            wakes_on_terrain_change,
            model: Vec::new(),
            authored_model,
            animation,
            interaction,
            behavior: Arc::new(
                crate::server::script::creature::ScriptCreature::client_authored(
                    max_private,
                    schema,
                ),
            ),
        };
        creature.validate().map_err(|_| invalid())?;
        startup.creatures.push(creature);
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    startup.creatures.sort_by(|a, b| a.key.cmp(&b.key));
    bundle.residency.resize(bytes.len())?;
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
fn float(reader: &mut Reader<'_>) -> Result<f32, ScriptError> {
    let f = f32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
    if f.is_finite() { Ok(f) } else { Err(invalid()) }
}
