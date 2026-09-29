//! V31 client-safe mobile model and identity. The server Luau module and
//! private state are never serialized into creature metadata.
use super::*;
use bloxgloom_host_api::entity::{Animation, Body, Cuboid, MobileEntity, PartMotion};
use std::sync::Arc;

pub(super) fn encode(
    writer: &mut Writer,
    owner: &str,
    declarations: &[MobileEntity],
) -> Result<(), ScriptError> {
    let mut own = declarations
        .iter()
        .filter(|d| {
            d.key
                .split_once(':')
                .is_some_and(|(namespace, _)| namespace == owner)
        })
        .collect::<Vec<_>>();
    own.sort_by(|a, b| a.key.cmp(&b.key));
    writer.count(own.len())?;
    for creature in own {
        creature.validate().map_err(|_| invalid())?;
        if creature.max_state_bytes < 13
            || creature.max_state_bytes > 268
            || creature.max_public_bytes != 5
            || creature.reads_neighbours
            || !creature.wakes_on_terrain_change
            || !creature.interaction.is_empty()
            || creature.model.len() > 16
            || creature.animation != Animation::default()
        {
            return Err(invalid());
        }
        writer.field(creature.key.as_bytes())?;
        writer.field(&creature.schema_version.to_le_bytes())?;
        writer.field(&creature.schema_fingerprint.to_le_bytes())?;
        writer.field(&((creature.max_state_bytes - 12) as u16).to_le_bytes())?;
        writer.field(&creature.interval.to_le_bytes())?;
        writer.field(&[creature.read_radius])?;
        for value in [
            creature.body.half_width,
            creature.body.height,
            creature.body.speed,
        ] {
            writer.field(&value.to_le_bytes())?;
        }
        writer.count(creature.model.len())?;
        for part in &creature.model {
            for value in part.min.into_iter().chain(part.max).chain(part.color) {
                writer.field(&value.to_le_bytes())?;
            }
            writer.field(&[match part.motion {
                PartMotion::Body => 0,
                PartMotion::LeftFoot => 1,
                PartMotion::RightFoot => 2,
            }])?;
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    owner: &str,
    requires: &[String],
) -> Result<Vec<MobileEntity>, ScriptError> {
    let mut result: Vec<MobileEntity> = Vec::new();
    for _ in 0..reader.count(8)? {
        if ![composition::CONTENT, composition::MOBILE_ENTITIES]
            .into_iter()
            .all(|capability| requires.iter().any(|r| r == capability))
        {
            return Err(invalid());
        }
        let key = reader.text(129)?;
        if key
            .split_once(':')
            .is_none_or(|(namespace, local)| namespace != owner || !identifier(local))
            || result.last().is_some_and(|old| old.key >= key)
        {
            return Err(invalid());
        }
        let schema_version =
            u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
        let schema_fingerprint =
            u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        let max_private = u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
        if !(1..=256).contains(&max_private) {
            return Err(invalid());
        }
        let interval = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
        let [read_radius] = reader.field(1)? else {
            return Err(invalid());
        };
        let body = Body {
            half_width: float(reader)?,
            height: float(reader)?,
            speed: float(reader)?,
        };
        let mut model = Vec::new();
        for _ in 0..reader.count(16)? {
            let mut values = [0.0; 9];
            for value in &mut values {
                *value = float(reader)?;
            }
            let [motion] = reader.field(1)? else {
                return Err(invalid());
            };
            model.push(Cuboid {
                min: values[..3].try_into().unwrap(),
                max: values[3..6].try_into().unwrap(),
                color: values[6..9].try_into().unwrap(),
                motion: match motion {
                    0 => PartMotion::Body,
                    1 => PartMotion::LeftFoot,
                    2 => PartMotion::RightFoot,
                    _ => return Err(invalid()),
                },
            });
        }
        let creature = MobileEntity {
            key,
            schema_version,
            schema_fingerprint,
            max_state_bytes: 12 + usize::from(max_private),
            max_public_bytes: 5,
            body,
            interval,
            read_radius: *read_radius,
            reads_neighbours: false,
            wakes_on_terrain_change: true,
            model,
            animation: Animation::default(),
            interaction: vec![],
            behavior: Arc::new(crate::server::script::creature::ScriptCreature::client(
                usize::from(max_private),
            )),
        };
        creature.validate().map_err(|_| invalid())?;
        if creature.interval > 1000 {
            return Err(invalid());
        }
        result.push(creature);
    }
    Ok(result)
}

fn float(reader: &mut Reader<'_>) -> Result<f32, ScriptError> {
    let value = f32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
    value.is_finite().then_some(value).ok_or_else(invalid)
}
