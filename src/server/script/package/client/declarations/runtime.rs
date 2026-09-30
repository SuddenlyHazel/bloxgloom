//! The startup adapter's closed metadata vocabulary. Actions are public UI/input
//! definitions; entities expose schema dimensions, not codecs or state. Handlers
//! and owner systems expose only their existing catalog compatibility identity.
//! The owner identity hashes seeds without disclosing them. Generation identity
//! is informational: generation has its own saved manifest, not a content ID.
use super::*;
use crate::content::client_metadata::{Entity, Identity};
use bloxgloom_host_api::{
    RegistrationError,
    actions::{
        Action, Command, CommandArgument, CommandPermission, MAX_COMMAND_ARGUMENTS, Operation,
        Target,
    },
};
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub(super) struct Runtime {
    actions: Vec<Action>,
    entities: Vec<Entity>,
    handlers: Vec<Identity>,
    systems: Vec<Identity>,
    players: Vec<Identity>,
    generation: Vec<(String, u32)>,
}

impl Runtime {
    pub(in crate::server::script::package::client) fn set_players(
        &mut self,
        identities: Vec<Identity>,
    ) {
        self.players = identities;
    }
    pub(super) fn counts(&self) -> [usize; 5] {
        [
            self.actions.len(),
            self.entities.len(),
            self.handlers.len(),
            self.systems.len(),
            self.generation.len(),
        ]
    }
    pub(super) fn project(
        d: &crate::server::script::startup::Declarations,
    ) -> Result<Self, ScriptError> {
        // Bound capture/sort work, not just the eventual output. These mirror
        // the startup host; a new binding must explicitly extend this format.
        if d.actions.len() > MAX_PACKAGES * 32
            || d.entities.len() > MAX_PACKAGES * 32
            || d.handlers.len() > MAX_PACKAGES * 32
            || d.systems.len() > MAX_PACKAGES
            || d.generation.len() > MAX_PACKAGES
        {
            return Err(invalid());
        }
        let mut result = Self::default();
        for (action, handler) in &d.actions {
            action.validate().map_err(|_| invalid())?;
            if action.operation != Operation::Gameplay || action.panel.is_some() {
                return Err(invalid());
            }
            result.actions.push(action.clone());
            result.handlers.push(Identity::new(
                b'G',
                handler.key.clone(),
                &handler.fingerprint_bytes(),
            ));
        }
        for handler in &d.handlers {
            result.handlers.push(Identity::new(
                b'G',
                handler.key.clone(),
                &handler.fingerprint_bytes(),
            ));
        }
        for entity in &d.entities {
            result.entities.push(Entity {
                key: entity.key.clone(),
                schema_version: entity.schema_version,
                schema_fingerprint: entity.schema_fingerprint,
                max_state_bytes: entity.max_state_bytes,
                initial_delay_ticks: entity.initial_delay_ticks,
            });
        }
        for system in &d.systems {
            // Seeds are private server initialization, not client declarations.
            // Bounds precede fingerprint allocation/traversal.
            if !(1..=4096).contains(&system.max_state_bytes)
                || !(1..=8).contains(&system.max_jobs_per_tick)
                || system.read_radius_chunks.is_some_and(|r| r > 1)
                || system.after.len() > 16
                || system.seeds.len() > 32
                || system
                    .seeds
                    .iter()
                    .any(|s| s.data.len() > system.max_state_bytes as usize)
            {
                return Err(invalid());
            }
            result.systems.push(Identity::new(
                b'Y',
                system.key.clone(),
                &system.fingerprint_bytes(),
            ));
        }
        for generator in &d.generation {
            result
                .generation
                .push((generator.key.clone(), generator.revision));
        }
        result.actions.sort_by(|a, b| a.key.cmp(&b.key));
        result.entities.sort_by(|a, b| a.key.cmp(&b.key));
        result.handlers.sort_by(|a, b| a.key.cmp(&b.key));
        result.systems.sort_by(|a, b| a.key.cmp(&b.key));
        result.generation.sort();
        Ok(result)
    }

    pub(super) fn encode_package(
        &self,
        writer: &mut Writer,
        name: &str,
    ) -> Result<(), ScriptError> {
        let own = |key: &str| key.split_once(':').is_some_and(|(owner, _)| owner == name);
        let actions = self
            .actions
            .iter()
            .filter(|a| own(&a.key))
            .collect::<Vec<_>>();
        writer.count(actions.len())?;
        for action in actions {
            action.validate().map_err(|_| invalid())?;
            writer.field(action.key.as_bytes())?;
            writer.count(action.version.into())?;
            writer.field(action.label.as_bytes())?;
            match &action.target {
                // New target tags preserve every existing non-command record.
                // Old decoders reject these rather than dropping authorization
                // or typed argument metadata. Zero-only tags 3/4 are retired.
                Target::Empty => writer.count(match &action.command {
                    None => 0,
                    Some(Command {
                        permission: CommandPermission::Player,
                        ..
                    }) => 5,
                    Some(Command {
                        permission: CommandPermission::Admin,
                        ..
                    }) => 6,
                })?,
                Target::Item(key) | Target::Block(key) => {
                    writer.count(if matches!(action.target, Target::Item(_)) {
                        1
                    } else {
                        2
                    })?;
                    writer.field(key.as_bytes())?;
                }
                Target::Entity(key) => {
                    // Tag 7 is new and rejects on older clients instead of
                    // silently turning entity authority into empty targeting.
                    writer.count(7)?;
                    writer.field(key.as_bytes())?;
                }
            }
            if let Some(command) = &action.command {
                writer.count(command.arguments.len())?;
                for argument in &command.arguments {
                    let (kind, bound) = match argument {
                        CommandArgument::Player => (3, 0),
                        CommandArgument::ItemKey { max_bytes } => (0, *max_bytes),
                        CommandArgument::EntityKey { max_bytes } => (1, *max_bytes),
                        CommandArgument::Count { default } => (2, default.unwrap_or(0)),
                    };
                    writer.count(kind)?;
                    writer.count(usize::from(bound))?;
                }
            }
        }
        let entities = self
            .entities
            .iter()
            .filter(|e| own(&e.key))
            .collect::<Vec<_>>();
        writer.count(entities.len())?;
        for e in entities {
            writer.field(e.key.as_bytes())?;
            writer.count(e.schema_version.into())?;
            writer.put(&e.schema_fingerprint.to_le_bytes())?;
            writer.count(e.max_state_bytes.into())?;
            writer.count(e.initial_delay_ticks.unwrap_or(0) as usize)?;
        }
        for identities in [&self.handlers, &self.systems] {
            let own = identities
                .iter()
                .filter(|i| own(&i.key))
                .collect::<Vec<_>>();
            writer.count(own.len())?;
            for identity in own {
                writer.field(identity.key.as_bytes())?;
                writer.put(&identity.fingerprint.to_le_bytes())?;
            }
        }
        let generation = self
            .generation
            .iter()
            .filter(|(key, _)| own(key))
            .collect::<Vec<_>>();
        writer.count(generation.len())?;
        for (key, revision) in generation {
            writer.field(key.as_bytes())?;
            writer.count(*revision as usize)?;
        }
        Ok(())
    }

    pub(super) fn decode_package(
        &mut self,
        reader: &mut Reader<'_>,
        name: &str,
        requires: &[String],
    ) -> Result<(), ScriptError> {
        let mut previous = String::new();
        let own_actions = count(reader, 32, requires, composition::ACTIONS)?;
        for _ in 0..own_actions {
            let key = own_key(reader, name, &mut previous)?;
            let version = reader.count(u16::MAX.into())? as u16;
            let label = reader.text(255)?;
            let kind = reader.count(7)?;
            let target = match kind {
                0 => Target::Empty,
                1 => Target::Item(reader.text(255)?),
                2 => Target::Block(reader.text(255)?),
                7 => Target::Entity(reader.text(255)?),
                5 | 6 => Target::Empty,
                _ => return Err(invalid()),
            };
            let command = if kind == 5 || kind == 6 {
                let count = reader.count(MAX_COMMAND_ARGUMENTS)?;
                let mut arguments = Vec::with_capacity(count);
                for _ in 0..count {
                    let kind = reader.count(3)?;
                    let bound = reader.count(128)? as u8;
                    arguments.push(match kind {
                        3 if bound == 0 => CommandArgument::Player,
                        0 => CommandArgument::ItemKey { max_bytes: bound },
                        1 => CommandArgument::EntityKey { max_bytes: bound },
                        2 => CommandArgument::Count {
                            default: (bound != 0).then_some(bound),
                        },
                        _ => return Err(invalid()),
                    });
                }
                Some(Command {
                    permission: if kind == 5 {
                        CommandPermission::Player
                    } else {
                        CommandPermission::Admin
                    },
                    arguments,
                })
            } else {
                None
            };
            let action = Action {
                key,
                version,
                label,
                target,
                operation: Operation::Gameplay,
                panel: None,
                command,
            };
            action.validate().map_err(|_| invalid())?;
            self.actions.push(action);
        }
        previous.clear();
        for _ in 0..count(reader, 32, requires, composition::ACTIONS)? {
            let entity = Entity {
                key: own_key(reader, name, &mut previous)?,
                schema_version: reader.count(u16::MAX.into())? as u16,
                schema_fingerprint: number(reader)?,
                max_state_bytes: reader.count(u16::MAX.into())? as u16,
                initial_delay_ticks: match reader.count(100_000)? {
                    0 => None,
                    n => Some(n as u32),
                },
            };
            if entity.schema_version == 0 || entity.max_state_bytes == 0 {
                return Err(invalid());
            }
            self.entities.push(entity);
        }
        for (kind, max, capability, identities) in [
            (
                b'G',
                32 + own_actions,
                composition::ACTIONS,
                &mut self.handlers,
            ),
            (b'Y', 1, composition::OWNER_SYSTEMS, &mut self.systems),
        ] {
            previous.clear();
            for _ in 0..count(reader, max, requires, capability)? {
                identities.push(Identity {
                    kind,
                    key: own_key(reader, name, &mut previous)?,
                    fingerprint: number(reader)?,
                });
            }
        }
        previous.clear();
        for _ in 0..count(reader, 1, requires, composition::GENERATION)? {
            let key = own_key(reader, name, &mut previous)?;
            let revision = reader.count(u32::MAX as usize)? as u32;
            if revision == 0 {
                return Err(invalid());
            }
            self.generation.push((key, revision));
        }
        Ok(())
    }

    pub(super) fn install(
        &self,
        catalog: &mut crate::content::Catalog,
    ) -> Result<(), RegistrationError> {
        for entity in &self.entities {
            catalog.client_entity(entity.clone())?;
        }
        for action in &self.actions {
            if !self.handlers.iter().any(|h| h.key == action.key) {
                return Err(RegistrationError(
                    "missing client action handler identity".into(),
                ));
            }
            catalog.register_action(action.clone())?;
        }
        for identity in self
            .handlers
            .iter()
            .chain(&self.systems)
            .chain(&self.players)
        {
            catalog.client_runtime_identity(identity.clone())?;
        }
        Ok(())
    }
}

fn count(
    reader: &mut Reader<'_>,
    max: usize,
    requires: &[String],
    capability: &str,
) -> Result<usize, ScriptError> {
    let count = reader.count(max)?;
    if count != 0 && !requires.iter().any(|r| r == capability) {
        return Err(invalid());
    }
    Ok(count)
}

fn own_key(
    reader: &mut Reader<'_>,
    name: &str,
    previous: &mut String,
) -> Result<String, ScriptError> {
    let key = reader.text(129)?;
    if key <= *previous
        || key
            .split_once(':')
            .is_none_or(|(owner, local)| owner != name || !identifier(local))
    {
        return Err(invalid());
    }
    previous.clone_from(&key);
    Ok(key)
}

fn number(reader: &mut Reader<'_>) -> Result<u64, ScriptError> {
    Ok(u64::from_le_bytes(
        reader.take(8)?.try_into().expect("eight bytes"),
    ))
}
