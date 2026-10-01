//! V45 transports closed moving-body/model declarations around an existing
//! artifact. Private records, callbacks and server source are never exported.
use super::*;
use bloxgloom_host_api::{
    RegistrationError,
    entity::{Cuboid, PartMotion},
    gameplay::EntityState,
    motion::{Body, CollisionMask, MovingEntity, Response},
};
use std::sync::Arc;
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x2d";
struct InertState;
impl EntityState for InertState {
    fn validate(&self, _data: &[u8]) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "server moving state unavailable in client metadata".into(),
        ))
    }
    fn public(&self, _data: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Err(RegistrationError(
            "server moving projection unavailable in client metadata".into(),
        ))
    }
}
pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.moving.is_empty() {
        return Ok(bundle);
    }
    if declarations.moving.len() > 128 {
        return Err(invalid());
    }
    let mut own = declarations.moving.iter().collect::<Vec<_>>();
    own.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(own.len())?;
    for d in own {
        encode(&mut writer, d)?;
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
    if inner.starts_with(MAGIC) {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(128)?;
    if count == 0 || !startup.moving.is_empty() {
        return Err(invalid());
    }
    let mut previous = String::new();
    let mut per_package = BTreeMap::<String, usize>::new();
    for _ in 0..count {
        let key = reader.text(129)?;
        let (owner, local) = key.split_once(':').ok_or_else(invalid)?;
        if key <= previous || !identifier(local) {
            return Err(invalid());
        }
        let package = startup
            .packages
            .iter()
            .find(|p| p.key == format!("{owner}:package"))
            .ok_or_else(invalid)?;
        if ![composition::CONTENT, composition::MOVING_ENTITIES]
            .iter()
            .all(|c| package.requires.iter().any(|r| r == c))
        {
            return Err(invalid());
        }
        let total = per_package.entry(owner.into()).or_default();
        *total += 1;
        if *total > 8 {
            return Err(invalid());
        }
        let schema_version = word(&mut reader)?;
        let schema_fingerprint =
            u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        let max_state_bytes = word(&mut reader)?;
        let max_public_bytes = word(&mut reader)?;
        let mut values = [0.0; 7];
        for v in &mut values {
            *v = float(&mut reader)?;
        }
        let [mask, response, flags] = reader.field(3)? else {
            return Err(invalid());
        };
        if mask & !7 != 0 || flags & !3 != 0 {
            return Err(invalid());
        }
        let body = Body {
            half_extents: values[..3].try_into().unwrap(),
            restitution: values[3],
            gravity_scale: values[4],
            max_speed: values[5],
            max_acceleration: values[6],
            collisions: CollisionMask {
                terrain: mask & 1 != 0,
                players: mask & 2 != 0,
                creatures: mask & 4 != 0,
            },
            response: match response {
                0 => Response::Stop,
                1 => Response::Bounce,
                2 => Response::Slide,
                _ => return Err(invalid()),
            },
        };
        let handles_impact = flags & 1 != 0;
        let handles_expiry = flags & 2 != 0;
        let lifetime_ticks = number(&mut reader)?;
        let interval = number(&mut reader)?;
        let source_exclusion_ticks = number(&mut reader)?;
        let mut model = Vec::new();
        for _ in 0..reader.count(64)? {
            let mut values = [0.0; 9];
            for v in &mut values {
                *v = float(&mut reader)?;
            }
            let [part] = reader.field(1)? else {
                return Err(invalid());
            };
            model.push(Cuboid {
                min: values[..3].try_into().unwrap(),
                max: values[3..6].try_into().unwrap(),
                color: values[6..9].try_into().unwrap(),
                motion: match part {
                    0 => PartMotion::Body,
                    1 => PartMotion::LeftFoot,
                    2 => PartMotion::RightFoot,
                    _ => return Err(invalid()),
                },
            });
        }
        let d = MovingEntity {
            key: key.clone(),
            schema_version,
            schema_fingerprint,
            max_state_bytes,
            max_public_bytes,
            body,
            lifetime_ticks,
            interval,
            source_exclusion_ticks,
            handles_impact,
            handles_expiry,
            model,
            state: Arc::new(InertState),
        };
        d.validate().map_err(|_| invalid())?;
        if startup.creatures.iter().any(|x| x.key == key) {
            return Err(invalid());
        }
        bundle.residency.add_payload(
            std::mem::size_of::<MovingEntity>()
                + d.key.len()
                + d.model.len() * std::mem::size_of::<Cuboid>(),
        )?;
        previous = key;
        startup.moving.push(d);
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    // Match existing wrapper residency: retire inner before the outer copy.
    bundle.residency.resize(bytes.len())?;
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
fn encode(writer: &mut Writer, d: &MovingEntity) -> Result<(), ScriptError> {
    d.validate().map_err(|_| invalid())?;
    writer.field(d.key.as_bytes())?;
    writer.field(&d.schema_version.to_le_bytes())?;
    writer.field(&d.schema_fingerprint.to_le_bytes())?;
    writer.field(&d.max_state_bytes.to_le_bytes())?;
    writer.field(&d.max_public_bytes.to_le_bytes())?;
    for x in d.body.half_extents.into_iter().chain([
        d.body.restitution,
        d.body.gravity_scale,
        d.body.max_speed,
        d.body.max_acceleration,
    ]) {
        writer.field(&x.to_le_bytes())?;
    }
    writer.field(&[
        d.body.collisions.terrain as u8
            | (d.body.collisions.players as u8) << 1
            | (d.body.collisions.creatures as u8) << 2,
        d.body.response as u8,
        d.handles_impact as u8 | (d.handles_expiry as u8) << 1,
    ])?;
    for x in [d.lifetime_ticks, d.interval, d.source_exclusion_ticks] {
        writer.field(&x.to_le_bytes())?;
    }
    writer.count(d.model.len())?;
    for p in &d.model {
        for x in p.min.into_iter().chain(p.max).chain(p.color) {
            writer.field(&x.to_le_bytes())?;
        }
        writer.field(&[p.motion as u8])?;
    }
    Ok(())
}
fn word(reader: &mut Reader<'_>) -> Result<u16, ScriptError> {
    Ok(u16::from_le_bytes(
        reader.field(2)?.try_into().map_err(|_| invalid())?,
    ))
}
fn number(reader: &mut Reader<'_>) -> Result<u32, ScriptError> {
    Ok(u32::from_le_bytes(
        reader.field(4)?.try_into().map_err(|_| invalid())?,
    ))
}
fn float(reader: &mut Reader<'_>) -> Result<f32, ScriptError> {
    let x = f32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
    if x.is_finite() { Ok(x) } else { Err(invalid()) }
}

#[cfg(test)]
mod tests;
