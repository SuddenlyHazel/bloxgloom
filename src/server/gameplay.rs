//! Translation from public gameplay plans to existing authoritative participants.
use super::durable::TerrainReads;
use crate::content::Catalog;
use crate::world::{BlockId, ChunkKey, PreparedEdit, World};
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error, Snapshot};
use std::io;
mod entities;
mod entity_inventory;
mod health;
pub(in crate::server) mod inventory;
mod motion;
mod player_inventory;
mod players;
mod profile_state;
mod teleport;
pub(in crate::server) use player_inventory::Capture as InventoryCapture;
pub(in crate::server) use players::{PlayerDecision, invoke as invoke_player};

pub(super) struct Participants<'a> {
    pub spawn_anchor: Option<[f32; 3]>,
    /// Original actor revision when native work precedes the callback overlay.
    pub actor_inventory_revision: Option<u64>,
    pub profile_inventories: Option<InventoryCapture<'a>>,
    pub profile_services: Option<&'a super::runtime::systems::SystemRuntime>,
    pub player_modifiers: Option<&'a super::players::modifiers::Runtime>,
    pub players: &'a [bloxgloom_host_api::gameplay::Player],
    pub action_id: Option<u128>,
    pub clock: Option<super::world_time::Capture>,
    pub weather: Option<super::weather::Capture>,
    pub actor: Option<(u128, &'a crate::inventory::Inventory)>,
    pub actor_position: Option<[f32; 3]>,
    pub admin: bool,
    pub entities: &'a super::entities::EntityStore,
}

pub(in crate::server) fn block(catalog: &Catalog, id: BlockId) -> Result<Block, Error> {
    let state = catalog
        .state(id)
        .ok_or_else(|| Error::Host("unknown stored block state".into()))?;
    let definition = catalog
        .block_type(state.block_type)
        .ok_or_else(|| Error::Host("unknown stored block type".into()))?;
    Ok(Block {
        state: state.key.clone(),
        block_type: definition.key.to_string(),
        primary_item: catalog
            .primary_block_item(id)
            .and_then(|id| catalog.item(id))
            .map(|item| item.key.to_string()),
        plant: catalog.block_flags(id) & crate::content::PLANT != 0,
        supports_plant: catalog.block_flags(id) & crate::content::SUPPORTS_PLANT != 0,
    })
}

pub(super) fn error(error: Error) -> io::Error {
    let kind = match error {
        Error::Unavailable(_) | Error::Deferred(_) => io::ErrorKind::WouldBlock,
        Error::Host(_) => io::ErrorKind::InvalidData,
        _ => io::ErrorKind::InvalidInput,
    };
    io::Error::new(kind, error)
}

struct WorldSnapshot<'a> {
    spawn_anchor: Option<[f32; 3]>,
    actor_inventory_revision: Option<u64>,
    profile_inventories: Option<InventoryCapture<'a>>,
    profile_inventory_before: std::collections::BTreeMap<u128, crate::inventory::Inventory>,
    profile_services: Option<&'a super::runtime::systems::SystemRuntime>,
    player_modifiers: Option<&'a super::players::modifiers::Runtime>,
    player_operations_enabled: bool,
    motion_reaction: Option<u64>,
    players: &'a [bloxgloom_host_api::gameplay::Player],
    action_id: Option<u128>,
    clock: Option<super::world_time::Capture>,
    weather: Option<super::weather::Capture>,
    world: &'a mut World,
    reads: &'a mut TerrainReads,
    requested: &'a mut Vec<ChunkKey>,
    actor: Option<(u128, &'a crate::inventory::Inventory)>,
    actor_position: Option<[f32; 3]>,
    admin: bool,
    inventory_read: bool,
    entities: Option<&'a super::entities::EntityStore>,
    tick: u64,
    seed: u64,
    origins: Vec<Cell>,
}
impl Snapshot for WorldSnapshot<'_> {
    fn native_respawn_position(&mut self) -> Result<[f32; 3], Error> {
        self.find_respawn()
    }

    fn player_health_cell(
        &mut self,
        profile: u128,
    ) -> Result<bloxgloom_host_api::gameplay::ProfileCell, Error> {
        if profile == 0 {
            return Err(Error::Invalid("invalid health profile".into()));
        }
        let runtime = self
            .profile_services
            .ok_or_else(|| Error::Invalid("health profile state unavailable".into()))?;
        let cell = super::players::health::capture_profile(runtime, profile)
            .map_err(|e| Error::Host(e.to_string()))?;
        self.reads
            .profile(
                &super::players::health::system(),
                profile,
                cell.initialized.then_some(cell.revision),
            )
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(cell)
    }
    fn damage_policies(&self) -> Vec<bloxgloom_host_api::player_health::DamageRegistration> {
        self.world.catalog().damage_policies().cloned().collect()
    }
    fn health_hooks(&self) -> Vec<bloxgloom_host_api::player_health::HookRegistration> {
        self.world.catalog().health_hooks().cloned().collect()
    }

    fn player_modifier_state(
        &mut self,
        namespace: &str,
        profile: u128,
        session: Option<u64>,
    ) -> Result<bloxgloom_host_api::player_modifiers::Capture, Error> {
        if profile == 0
            || !self.world.catalog().player_authority(namespace)
            || !self.player_operations_enabled
        {
            return Err(Error::Invalid("player modifier authority denied".into()));
        }
        if session.is_some_and(|session| {
            !self.players.iter().any(|player| {
                player.profile == profile && player.session == session && session != 0
            })
        }) {
            return Err(Error::Invalid("modifier session is not online".into()));
        }
        let runtime = self
            .profile_services
            .ok_or_else(|| Error::Invalid("modifier profile state unavailable".into()))?;
        let cell = super::players::modifiers::capture_profile(runtime, profile)
            .map_err(|e| Error::Host(e.to_string()))?;
        self.reads
            .profile(
                &super::players::modifiers::system(),
                profile,
                cell.initialized.then_some(cell.revision),
            )
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let session = match session {
            Some(session) => self
                .player_modifiers
                .ok_or_else(|| Error::Invalid("modifier session state unavailable".into()))?
                .capture(profile, session),
            None => Default::default(),
        };
        Ok(bloxgloom_host_api::player_modifiers::Capture {
            profile: cell,
            session,
        })
    }
    fn tags(&self) -> Option<std::sync::Arc<dyn bloxgloom_host_api::queries::Tags>> {
        Some(self.world.catalog_arc())
    }

    fn authorize_inventory(
        &self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
        namespace: Option<&str>,
    ) -> Result<(), Error> {
        if let bloxgloom_host_api::gameplay::InventoryId::Player(profile) = owner
            && self.actor.is_none_or(|(actor, _)| actor != profile)
            && (profile == 0
                || self.profile_inventories.is_none()
                || !namespace.is_some_and(|ns| self.player_authority(ns)))
        {
            return Err(Error::Invalid("profile inventory authority denied".into()));
        }
        Ok(())
    }
    fn profile_state(
        &mut self,
        namespace: &str,
        key: &str,
        profile: u128,
    ) -> Result<bloxgloom_host_api::gameplay::ProfileCell, Error> {
        self.capture_profile(namespace, key, profile)
    }
    fn validate_profile_state(
        &self,
        namespace: &str,
        key: &str,
        state: &bloxgloom_host_api::players::State,
    ) -> Result<(), Error> {
        self.validate_profile(namespace, key, state)
    }
    fn player_model_schema(&self, key: &str) -> Option<bloxgloom_host_api::entity::VisualSchema> {
        let catalog = self.world.catalog();
        catalog
            .player_model(catalog.player_model_id(key)?)
            .map(|model| model.schema())
    }
    fn valid_player_appearance(&self, palettes: [u8; 3]) -> bool {
        self.world
            .catalog()
            .valid_appearance([palettes[0], palettes[1], palettes[2], 0])
    }
    fn player_authority(&self, namespace: &str) -> bool {
        self.player_operations_enabled && self.world.catalog().player_authority(namespace)
    }
    fn players(&mut self) -> Result<Vec<bloxgloom_host_api::gameplay::Player>, Error> {
        Ok(self.players.to_vec())
    }
    fn action_id(&self) -> Option<u128> {
        self.action_id
    }
    fn sound_registered(&self, key: &str) -> bool {
        self.world.catalog().sounds.contains(key)
    }
    fn weather(&mut self) -> Result<bloxgloom_host_api::gameplay::Weather, Error> {
        let capture = self
            .weather
            .as_ref()
            .ok_or_else(|| Error::Invalid("weather unavailable in this context".into()))?;
        self.reads.weather = Some(capture.stamp.clone());
        Ok(capture.weather)
    }
    fn world_time(&mut self) -> Result<bloxgloom_host_api::gameplay::WorldTime, Error> {
        let clock = self
            .clock
            .as_ref()
            .ok_or_else(|| Error::Invalid("world clock unavailable in this context".into()))?;
        self.reads.clock = Some(clock.stamp.clone());
        Ok(clock.time)
    }
    fn pickup_eligible(&self, drop_id: u64) -> bool {
        self.actor_position
            .filter(|_| self.actor.is_some())
            .zip(self.entities)
            .is_some_and(|(position, store)| {
                super::drops::pickup_eligible(store, self.world.catalog(), drop_id, position)
            })
    }
    fn player_position(&self) -> Option<[f32; 3]> {
        self.actor.and(self.actor_position)
    }
    fn admin(&self) -> bool {
        self.admin && self.actor.is_some_and(|(profile, _)| profile != 0)
    }
    fn tick(&self) -> u64 {
        self.tick
    }
    fn seed(&self) -> u64 {
        self.seed
    }
    fn validate_entity_schedule(&self, raw_id: u64) -> Result<(), Error> {
        let id = super::entities::EntityId::new(raw_id)
            .ok_or_else(|| Error::Invalid("invalid entity ID".into()))?;
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        let snapshot = store
            .snapshot(id)
            .ok_or_else(|| Error::Invalid("scheduled entity gone".into()))?;
        let definition = self
            .world
            .catalog()
            .entity_type(snapshot.entity_type)
            .ok_or_else(|| Error::Host("unknown scheduled entity type".into()))?;
        if self
            .world
            .catalog()
            .gameplay_handler(
                bloxgloom_host_api::gameplay::EventKind::EntityTick,
                &definition.key,
            )
            .is_none()
        {
            return Err(Error::Invalid(format!(
                "{} has no registered tick handler",
                definition.key
            )));
        }
        Ok(())
    }
    fn project_entity_state(&self, id: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::project(self.world.catalog(), store, id, state)
    }
    fn nearby_entities(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Entity>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::nearby(self.world.catalog(), self.reads, store, position, radius)
    }
    fn entity_state(&mut self, id: u64, owner: &str) -> Result<Option<Vec<u8>>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::state(self.world.catalog(), self.reads, store, id, owner)
    }
    fn motion_contact(
        &mut self,
        id: u64,
        owner: &str,
    ) -> Result<Option<bloxgloom_host_api::motion::MotionContact>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("motion capture unavailable".into()))?;
        super::entities::motion::services::read_contact(
            self.world.catalog(),
            self.reads,
            store,
            id,
            owner,
        )
    }
    fn motion(
        &mut self,
        id: u64,
        owner: &str,
    ) -> Result<Option<bloxgloom_host_api::motion::Motion>, Error> {
        self.capture_motion(id, owner)
    }
    fn validate_moving_spawn(
        &mut self,
        owner: &str,
        spawn: &bloxgloom_host_api::gameplay::MovingSpawn,
    ) -> Result<(), Error> {
        self.validate_motion_spawn(owner, spawn)
    }
    fn validate_motion_change(
        &self,
        id: u64,
        owner: &str,
        value: &bloxgloom_host_api::motion::Motion,
    ) -> Result<(), Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("motion capture unavailable".into()))?;
        let Some((record, key)) =
            super::entities::motion::services::record(self.world.catalog(), store, id, owner)?
        else {
            return Err(Error::Invalid("moving entity gone".into()));
        };
        if record.pending.is_some() && self.motion_reaction != Some(id) {
            return Err(Error::Invalid(
                "motion is paused for a pending reaction".into(),
            ));
        }
        let declaration =
            super::entities::motion::services::declaration(self.world.catalog(), &key, owner)?;
        if declaration.physics.is_some() && value.orientation != record.motion.orientation {
            return Err(Error::Invalid(
                "rigid-body orientation is controlled by angular velocity".into(),
            ));
        }
        crate::content::moving::validate_motion(declaration, value).map_err(|e| Error::Invalid(e.0))
    }
    fn validate_entity_state(&self, key: &str, owner: &str, state: &[u8]) -> Result<(), Error> {
        entities::validate_state(self.world.catalog(), key, owner, state)
    }
    fn entity(&mut self, id: u64) -> Result<Option<bloxgloom_host_api::gameplay::Entity>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::read(self.world.catalog(), self.reads, store, id)
    }
    fn anchored_entity_at(&mut self, cell: Cell) -> Result<Option<u64>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
        entities::anchored(self.reads, store, cell)
    }
    fn player(&self) -> Option<u128> {
        self.actor.map(|(profile, _)| profile)
    }
    fn inventory(
        &mut self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Slot>, Error> {
        if let bloxgloom_host_api::gameplay::InventoryId::Entity(id) = owner {
            let store = self.entities.ok_or(Error::InventoryUnavailable(owner))?;
            let entity_id =
                super::entities::EntityId::new(id).ok_or(Error::InventoryUnavailable(owner))?;
            let snapshot = store
                .snapshot(entity_id)
                .ok_or(Error::InventoryUnavailable(owner))?;
            let cell = match snapshot.location {
                super::entities::EntityLocation::Anchored { anchor, .. } => {
                    [anchor.x, anchor.y, anchor.z]
                }
                super::entities::EntityLocation::Mobile { position } => {
                    let cell = super::entities::position_to_cell(position)
                        .map_err(|_| Error::Invalid("invalid entity inventory position".into()))?;
                    [cell.x, cell.y, cell.z]
                }
            };
            if !self.origins.iter().any(|origin| {
                (0..3).all(|axis| (i64::from(cell[axis]) - i64::from(origin[axis])).abs() <= 8)
            }) {
                return Err(Error::Invalid(
                    "entity inventory outside interaction radius".into(),
                ));
            }
            return entity_inventory::capture(self.world.catalog(), self.reads, store, id);
        }
        let bloxgloom_host_api::gameplay::InventoryId::Player(profile) = owner else {
            unreachable!()
        };
        let value =
            if let Some((actor, inventory)) = self.actor.filter(|(actor, _)| *actor == profile) {
                self.inventory_read = true;
                self.reads
                    .inventory(
                        actor,
                        self.actor_inventory_revision.unwrap_or(inventory.revision),
                    )
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                inventory.clone()
            } else {
                self.capture_inventory(profile)?
            };
        inventory::capture(self.world.catalog(), &value)
    }
    fn validate_stack(&self, stack: &bloxgloom_host_api::gameplay::Stack) -> Result<(), Error> {
        inventory::stack(self.world.catalog(), stack).map(|_| ())
    }
    fn inventory_accepts(
        &self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
        slot: usize,
        stack: &bloxgloom_host_api::gameplay::Stack,
    ) -> bool {
        match owner {
            bloxgloom_host_api::gameplay::InventoryId::Player(profile) => {
                self.actor
                    .is_some_and(|(id, inventory)| id == profile && slot < inventory.slots.len())
                    || self
                        .profile_inventory_before
                        .get(&profile)
                        .is_some_and(|inventory| slot < inventory.slots.len())
            }
            bloxgloom_host_api::gameplay::InventoryId::Entity(id) => {
                self.entities.is_some_and(|store| {
                    entity_inventory::accepts(self.world.catalog(), store, id, slot, stack)
                })
            }
        }
    }
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        let [x, y, z] = cell;
        let id = self
            .reads
            .read(self.world, x, y, z)
            .map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock => Error::Unavailable(cell),
                io::ErrorKind::QuotaExceeded => Error::BudgetExceeded,
                _ => Error::Host(e.to_string()),
            })?;
        let Some(id) = id else {
            let key = crate::world::world_to_chunk(x, y, z).0;
            if !self.requested.contains(&key) {
                self.requested.push(key);
            }
            return Err(Error::Unavailable(cell));
        };
        block(self.world.catalog(), id)
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        let catalog = self.world.catalog();
        block(
            catalog,
            catalog
                .state_by_key(key)
                .ok_or_else(|| Error::UnknownContent(key.into()))?,
        )
    }
    fn item_exists(&self, key: &str) -> bool {
        self.world.catalog().item_by_key(key).is_some()
    }
}

pub(super) type Edit = (i32, i32, i32, BlockId);
pub(super) type Spawn = ([f32; 3], crate::inventory::Stack, std::time::Duration);
pub(super) type Removal = (BlockId, Cell, bloxgloom_host_api::gameplay::RemovalCause);

pub(super) struct OperationInput<'a> {
    pub edits: &'a [Edit],
    pub removals: &'a [Removal],
    pub seed: u64,
    pub tick: u64,
    pub action: Option<bloxgloom_host_api::gameplay::Event>,
}

pub(super) struct WorldPlan {
    pub sounds: Vec<bloxgloom_host_api::sound::Event>,
    pub profile_inventory_changes: Vec<crate::server::journal::Change>,
    pub profile_states:
        std::collections::BTreeMap<(String, u128), bloxgloom_host_api::gameplay::ProfileCell>,
    pub player_operations: Vec<bloxgloom_host_api::gameplay::PlayerOperation>,
    pub world_time: Option<u64>,
    pub weather: Option<(u8, u32)>,
    pub entity_updates: Vec<super::entities::PreparedEntityTransaction>,
    pub entity_spawns: Vec<super::entities::EntitySpawn>,
    pub edits: Vec<Edit>,
    pub prepared: Vec<PreparedEdit>,
    pub drops: Vec<Spawn>,
    pub inventory: Option<crate::inventory::Inventory>,
    pub drop_takes: Vec<(u64, u16)>,
    pub admin_spawns: Vec<String>,
}

/// Invoke decision owners with one shared overlay. Every terrain/drop effect is
/// translated into participants of the caller's existing CommitAction.
pub(super) fn plan_removals(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    input: OperationInput<'_>,
    participants: Participants<'_>,
) -> io::Result<WorldPlan> {
    plan_with_lifecycles(world, reads, requested, input, participants, None)
}

/// Action/tick world effects opt into complete anchored lifecycle expansion.
/// Legacy block/owner/burn callers retain their existing dispatch contracts.
pub(super) fn plan_with_lifecycles(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    input: OperationInput<'_>,
    participants: Participants<'_>,
    lifecycles: Option<&super::lifecycle::Registry>,
) -> io::Result<WorldPlan> {
    use bloxgloom_host_api::gameplay::{Event, EventKind, cell_random};
    let OperationInput {
        edits,
        removals,
        seed,
        tick,
        action,
    } = input;
    let actor = participants.actor;
    let mut origins = edits
        .iter()
        .map(|&(x, y, z, _)| [x, y, z])
        .collect::<Vec<_>>();
    if let Some(event) = &action {
        match event {
            Event::ActionRequested {
                cell: Some(cell), ..
            } => origins.push(*cell),
            Event::ActionRequested { position, .. }
            | Event::EntityTick { position, .. }
            | Event::PickupRequested { position, .. } => {
                let cell = super::entities::position_to_cell(*position).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid event position")
                })?;
                origins.push([cell.x, cell.y, cell.z]);
            }
            Event::MovingTick { motion, .. } => {
                let cell = motion.position.map(|x| x.floor() as i32);
                origins.push(cell);
            }
            Event::MovingImpact { impact } => {
                origins.push(impact.position.map(|x| x.floor() as i32));
            }
            Event::MovingExpiry { entity, .. } => {
                if let Some(s) = super::entities::EntityId::new(*entity)
                    .and_then(|id| participants.entities.snapshot(id))
                    && let super::entities::EntityLocation::Mobile { position } = s.location
                {
                    origins.push(position.map(|x| x.floor() as i32));
                }
            }
            _ => {}
        }
    }
    let catalog = world.catalog_arc();
    // Preparation is invisible. Its per-chunk version is stable random input
    // for existing harvest behavior; an expanded overlay is prepared below.
    let prepared = world.prepare_edits(edits)?;
    let mut snapshot = WorldSnapshot {
        spawn_anchor: participants.spawn_anchor,
        actor_inventory_revision: participants.actor_inventory_revision,
        profile_inventories: participants.profile_inventories,
        profile_inventory_before: Default::default(),
        profile_services: participants.profile_services,
        player_modifiers: participants.player_modifiers,
        player_operations_enabled: matches!(
            &action,
            Some(
                Event::ActionRequested { .. }
                    | Event::MovingTick { .. }
                    | Event::MovingImpact { .. }
                    | Event::MovingExpiry { .. }
            )
        ),
        motion_reaction: match &action {
            Some(Event::MovingTick { entity, .. } | Event::MovingExpiry { entity, .. }) => {
                Some(*entity)
            }
            Some(Event::MovingImpact { impact }) => Some(impact.entity),
            _ => None,
        },
        players: participants.players,
        action_id: participants.action_id,
        clock: participants.clock,
        weather: participants.weather,
        world,
        reads,
        requested,
        actor,
        actor_position: participants.actor_position,
        admin: participants.admin,
        inventory_read: false,
        entities: Some(participants.entities),
        tick,
        seed,
        origins,
    };
    if let Some((profile, _)) = actor
        && snapshot.profile_services.is_some()
    {
        let health = snapshot.player_health_cell(profile).map_err(error)?;
        let health = bloxgloom_host_api::player_health::State::decode(&health.state.data)
            .map_err(io::Error::other)?;
        let respawn = matches!(&action,Some(Event::ActionRequested{action,..}) if action==crate::gameplay::respawn::KEY);
        if !health.alive() && !respawn {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "player is dead",
            ));
        }
    }
    let mut context = Context::new(&mut snapshot, 4096);
    let mut placements = Vec::new();
    for &(x, y, z, state) in edits {
        let state = catalog
            .state(state)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown block state"))?;
        let previous = context.block([x, y, z]).map_err(error)?;
        context.set_block([x, y, z], &state.key).map_err(error)?;
        if previous.state != state.key && state.id != crate::world::AIR {
            placements.push((
                [x, y, z],
                previous,
                block(&catalog, state.id).map_err(error)?,
            ));
        }
    }
    for &(id, cell, cause) in removals {
        let previous = block(&catalog, id).map_err(error)?;
        let Some(handler) = catalog.gameplay_handler(EventKind::BlockRemoved, &previous.block_type)
        else {
            continue;
        };
        let key = crate::world::world_to_chunk(cell[0], cell[1], cell[2]).0;
        let version = prepared
            .iter()
            .find(|edit| edit.key == key)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "removal outside staged edits")
            })?
            .new_version;
        let event = Event::BlockRemoved {
            cell,
            previous,
            cause,
            random: cell_random(seed, cell, version),
        };
        context.dispatch(handler, &event).map_err(|e| {
            let e = error(e);
            io::Error::new(e.kind(), format!("{}: {e}", handler.key))
        })?;
    }
    // Every original edit is staged first. Removal decisions run before
    // placement decisions; both see the same read-your-writes overlay. Handler-
    // emitted edits are effects, not recursively dispatched new decisions.
    for (cell, previous, placed) in placements {
        let Some(handler) = catalog.gameplay_handler(EventKind::BlockPlaced, &placed.block_type)
        else {
            continue;
        };
        let event = Event::BlockPlaced {
            cell,
            previous,
            placed,
        };
        context.dispatch(handler, &event).map_err(|e| {
            let e = error(e);
            io::Error::new(e.kind(), format!("{}: {e}", handler.key))
        })?;
    }
    if let Some(event) = &action {
        let (kind, target) = match event {
            Event::ActionRequested { action, .. } => (EventKind::ActionRequested, action.as_str()),
            Event::EntityTick { entity, .. }
            | Event::MovingTick { entity, .. }
            | Event::MovingExpiry { entity, .. }
            | Event::MovingImpact {
                impact: bloxgloom_host_api::motion::Impact { entity, .. },
            } => {
                let id = super::entities::EntityId::new(*entity).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid scheduled entity")
                })?;
                let snapshot = participants.entities.snapshot(id).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::WouldBlock, "scheduled entity gone")
                })?;
                let key = &catalog
                    .entity_type(snapshot.entity_type)
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "scheduled type gone")
                    })?
                    .key;
                (
                    match event {
                        Event::MovingTick { .. } => EventKind::MovingTick,
                        Event::MovingImpact { .. } => EventKind::MovingImpact,
                        Event::MovingExpiry { .. } => EventKind::MovingExpiry,
                        _ => EventKind::EntityTick,
                    },
                    key.as_ref(),
                )
            }
            Event::PickupRequested { .. } => (EventKind::PickupRequested, "bloxgloom:drop"),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid additional gameplay decision",
                ));
            }
        };
        let handler = catalog.gameplay_handler(kind, target).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "gameplay decision handler missing",
            )
        })?;
        context.dispatch(handler, event).map_err(|e| {
            let e = error(e);
            io::Error::new(e.kind(), format!("{}: {e}", handler.key))
        })?;
    }
    let mut expansion = super::durable::actions::invalidation::gameplay::Expansion::default();
    dispatch_neighbors(
        &catalog,
        &mut context,
        edits,
        seed,
        tick,
        lifecycles.map(|registry| (&mut expansion, participants.entities, registry)),
    )?;
    let mut plan = context.finish().map_err(error)?;
    expansion.validate(&plan)?;
    if let Some(Event::EntityTick { entity, .. }) = &action
        && !matches!(
            plan.entity_changes.get(entity),
            Some(EntityChange::Remove { .. })
        )
    {
        plan.entity_schedules.entry(*entity).or_insert(None);
    }
    let profile_inventory_changes = player_inventory::prepare(
        &catalog,
        std::mem::take(&mut snapshot.profile_inventory_before),
        &mut plan.inventories,
    )?;
    let inventory = if snapshot.inventory_read {
        let (profile, before) = actor.expect("inventory read requires actor capture");
        let owner = bloxgloom_host_api::gameplay::InventoryId::Player(profile);
        let after = match plan.inventories.remove(&owner) {
            Some(slots) => inventory::apply(&catalog, before, slots).map_err(error)?,
            None => before.clone(),
        };
        (after != *before).then_some(after)
    } else {
        None
    };
    motion::prepare(
        &catalog,
        participants.entities,
        &mut plan,
        tick,
        action.as_ref(),
    )?;
    let mut entity_updates = Vec::new();
    let mut drop_takes = Vec::new();
    for (owner, slots) in plan.inventories {
        let bloxgloom_host_api::gameplay::InventoryId::Entity(id) = owner else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "uncaptured gameplay inventory output",
            ));
        };
        let entity_id = super::entities::EntityId::new(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid drop owner"))?;
        if participants
            .entities
            .snapshot(entity_id)
            .is_some_and(|snapshot| snapshot.entity_type == super::drops::DROP_ENTITY_TYPE)
        {
            let before = super::drops::stack(participants.entities, entity_id)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "drop stack vanished"))?;
            let remaining = slots
                .first()
                .and_then(Option::as_ref)
                .map_or(0, |stack| stack.count);
            if remaining < before.count {
                drop_takes.push((id, before.count - remaining));
            }
        }
        if let Some(snapshot) = expansion.snapshots.get_mut(&entity_id) {
            // Validate the staged transfer exactly as for surviving entities,
            // but refund from its final payload and submit only the despawn.
            entity_inventory::prepare(&catalog, reads, participants.entities, id, slots.clone())
                .map_err(error)?;
            let policy = participants
                .entities
                .types()
                .descriptor(snapshot.entity_type)
                .map_err(io::Error::other)?
                .transfer_policy()
                .ok_or_else(|| io::Error::other("invalidated inventory has no transfer policy"))?;
            let slots = slots
                .iter()
                .map(|slot| {
                    slot.as_ref()
                        .map(|stack| inventory::stack(&catalog, stack))
                        .transpose()
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?;
            snapshot.private_payload = policy
                .replace_inventory(&snapshot.private_payload, slots, &catalog)
                .map_err(io::Error::other)?;
            continue;
        }
        if let Some(update) =
            entity_inventory::prepare(&catalog, reads, participants.entities, id, slots)
                .map_err(error)?
        {
            entity_updates.push(update);
        }
    }
    use bloxgloom_host_api::gameplay::EntityChange;
    for (id, change) in plan.entity_changes {
        let id = super::entities::EntityId::new(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid entity ID"))?;
        let before = participants.entities.snapshot(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::WouldBlock, "entity changed during planning")
        })?;
        let transaction = match change {
            EntityChange::Update { state, .. } => participants.entities.prepare_update(
                id,
                before.revision,
                super::entities::EntityPatch {
                    payload: Some(super::entities::EntityPayload::new(state)),
                    next_tick: plan.entity_schedules.remove(&id.get()),
                    ..Default::default()
                },
            ),
            EntityChange::Remove { .. } => {
                participants.entities.prepare_despawn(id, before.revision)
            }
        }
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        entity_updates.push(transaction);
    }
    for (raw_id, due) in plan.entity_schedules {
        let id = super::entities::EntityId::new(raw_id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid scheduled entity")
        })?;
        let before = participants
            .entities
            .snapshot(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "scheduled entity gone"))?;
        entity_updates.push(
            participants
                .entities
                .prepare_update(
                    id,
                    before.revision,
                    super::entities::EntityPatch {
                        next_tick: Some(due),
                        ..Default::default()
                    },
                )
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
        );
    }
    let final_edits = plan
        .blocks
        .into_iter()
        .map(|([x, y, z], key)| {
            catalog
                .state_by_key(&key)
                .map(|id| (x, y, z, id))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "staged state disappeared")
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut original = edits.to_vec();
    original.sort_by_key(|&(x, y, z, _)| [x, y, z]);
    let mut sources = removals
        .iter()
        .map(|(_, cell, _)| *cell)
        .chain(edits.iter().map(|&(x, y, z, _)| [x, y, z]))
        .collect::<Vec<_>>();
    if let Some(event) = &action {
        let origin = match event {
            Event::ActionRequested {
                cell: Some(cell), ..
            } => *cell,
            Event::ActionRequested { position, .. }
            | Event::EntityTick { position, .. }
            | Event::PickupRequested { position, .. } => {
                let at = super::entities::position_to_cell(*position).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid event position")
                })?;
                [at.x, at.y, at.z]
            }
            Event::MovingTick { motion, .. } => motion.position.map(|x| x.floor() as i32),
            Event::MovingImpact { impact } => impact.position.map(|x| x.floor() as i32),
            Event::MovingExpiry { entity, .. } => {
                let id = super::entities::EntityId::new(*entity).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid expiry entity")
                })?;
                let snapshot = participants.entities.snapshot(id).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::WouldBlock, "expiry entity disappeared")
                })?;
                let super::entities::EntityLocation::Mobile { position } = snapshot.location else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "expiry entity is anchored",
                    ));
                };
                position.map(|x| x.floor() as i32)
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid gameplay event",
                ));
            }
        };
        sources.push(origin);
    }
    let within_reach = |cell: Cell| {
        sources.iter().any(|source| {
            (0..3).all(|axis| (i64::from(cell[axis]) - i64::from(source[axis])).abs() <= 8)
        })
    };
    if final_edits
        .iter()
        .any(|&(x, y, z, _)| !within_reach([x, y, z]))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "gameplay edit outside interaction radius",
        ));
    }
    let prepared = if final_edits == original {
        prepared
    } else {
        world.prepare_edits(&final_edits)?
    };
    teleport::validate(
        world,
        reads,
        requested,
        &plan.player_operations,
        &final_edits,
    )?;
    let mut entity_spawns = Vec::with_capacity(plan.entity_spawns.len());
    for spawn in plan.entity_spawns {
        let id = catalog.entity_type_id_by_key(&spawn.key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "unknown gameplay entity")
        })?;
        let at = super::entities::position_to_cell(spawn.position).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid gameplay entity position",
            )
        })?;
        let target = [at.x, at.y, at.z];
        if !within_reach(target) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "entity spawn outside interaction radius",
            ));
        }
        let state = if let Some(&(_, _, _, state)) = final_edits
            .iter()
            .find(|&&(x, y, z, _)| [x, y, z] == target)
        {
            state
        } else {
            let [x, y, z] = target;
            let Some(state) = reads.read(world, x, y, z)? else {
                let key = crate::world::world_to_chunk(x, y, z).0;
                if !requested.contains(&key) {
                    requested.push(key);
                }
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "entity spawn terrain unavailable",
                ));
            };
            state
        };
        if catalog.block_flags(state) & crate::content::SOLID != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "entity spawn is obstructed",
            ));
        }
        if let Some(moving) = catalog.moving_entity(id) {
            motion::validate_spawn_volume(
                world,
                reads,
                requested,
                &catalog,
                &final_edits,
                moving.capture_half_extents(),
                spawn.position,
            )?;
        }
        entity_spawns.push(super::entities::EntitySpawn::Mobile {
            entity_type: id,
            position: spawn.position,
            payload: super::entities::EntityPayload::new(spawn.state),
            spawn_tick: tick,
        });
    }
    let mut drops = plan
        .drops
        .into_iter()
        .map(|drop| {
            let at = super::entities::position_to_cell(drop.position).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid gameplay drop position",
                )
            })?;
            if !within_reach([at.x, at.y, at.z]) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "gameplay drop outside interaction radius",
                ));
            }
            Ok((
                drop.position,
                inventory::stack(&catalog, &drop.stack).map_err(error)?,
                std::time::Duration::from_millis(u64::from(drop.pickup_delay_ms)),
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    if let Some(lifecycles) = lifecycles {
        let (refunds, despawns) = expansion.finish(&catalog, lifecycles, participants.entities)?;
        drops.extend(refunds);
        entity_updates.extend(despawns);
    }
    Ok(WorldPlan {
        sounds: plan.sounds,
        profile_inventory_changes,
        profile_states: plan.profile_states,
        player_operations: plan.player_operations,
        world_time: plan.world_time,
        weather: plan.weather,
        entity_updates,
        entity_spawns,
        edits: final_edits,
        prepared,
        drops,
        inventory,
        drop_takes,
        admin_spawns: plan.admin_spawns,
    })
}

/// Run support and registered neighbor decisions on the same staged overlay,
/// including edits created by another handler. Only the upward support path
/// uses the fallback; targeted handlers observe any of the six adjacent cells.
fn dispatch_neighbors(
    catalog: &std::sync::Arc<Catalog>,
    context: &mut Context<'_>,
    edits: &[Edit],
    seed: u64,
    tick: u64,
    mut lifecycle: Option<(
        &mut super::durable::actions::invalidation::gameplay::Expansion,
        &super::entities::EntityStore,
        &super::lifecycle::Registry,
    )>,
) -> io::Result<()> {
    use bloxgloom_host_api::gameplay::{Event, EventKind, RemovalCause, cell_random};
    use std::collections::BTreeSet;
    let mut seen = BTreeSet::new();
    let mut dispatched_removals = BTreeSet::new();
    let mut processed = std::collections::BTreeMap::new();
    let original_edits: BTreeSet<_> = edits.iter().map(|&(x, y, z, _)| [x, y, z]).collect();
    let targeted = catalog.has_targeted_neighbor_handlers();
    loop {
        if let Some((expansion, store, registry)) = &mut lifecycle {
            expansion.expand(context, catalog, store, registry, seed, tick)?;
        }
        let pending: Vec<_> = context
            .staged_block_transitions()
            .into_iter()
            .filter(|(cell, _, _)| !seen.contains(cell))
            .collect();
        if pending.is_empty() {
            if context
                .staged_block_transitions()
                .iter()
                .any(|(cell, _, after)| processed.get(cell).is_some_and(|value| value != after))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "neighbor handler rewrote an already dispatched block",
                ));
            }
            return Ok(());
        }
        if seen.len() + pending.len() > 256 {
            return Err(io::Error::new(
                io::ErrorKind::QuotaExceeded,
                "neighbor decision chain exceeds 256 edits",
            ));
        }
        for (changed, previous, current) in pending {
            seen.insert(changed);
            processed.insert(changed, current.clone());
            if !original_edits.contains(&changed)
                && !lifecycle
                    .as_ref()
                    .is_some_and(|(expansion, _, _)| expansion.cells.contains_key(&changed))
                && !dispatched_removals.contains(&changed)
                && previous.block_type != "bloxgloom:air"
                && previous != current
            {
                let event = Event::BlockRemoved {
                    cell: changed,
                    previous: previous.clone(),
                    cause: if context.player().is_none() {
                        RemovalCause::WorldEdit
                    } else if current.block_type == "bloxgloom:air" {
                        RemovalCause::Break
                    } else {
                        RemovalCause::Replacement
                    },
                    random: cell_random(seed, changed, tick),
                };
                if let Some(handler) =
                    catalog.gameplay_handler(EventKind::BlockRemoved, &previous.block_type)
                {
                    context.dispatch(handler, &event).map_err(error)?;
                }
            }
            let lost_support = previous.supports_plant && !current.supports_plant;
            if !targeted && !lost_support {
                continue;
            }
            for offset in [
                [0, 1, 0],
                [0, -1, 0],
                [1, 0, 0],
                [-1, 0, 0],
                [0, 0, 1],
                [0, 0, -1],
            ] {
                let Some(cell) = (0..3)
                    .map(|axis| changed[axis].checked_add(offset[axis]))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let cell: Cell = cell.try_into().expect("three axes");
                let above = offset == [0, 1, 0] && lost_support;
                if !targeted && !above {
                    continue;
                }
                let neighbor = context.block(cell).map_err(error)?;
                if neighbor.block_type == "bloxgloom:air" {
                    continue;
                }
                let Some(handler) =
                    catalog.gameplay_handler(EventKind::NeighborChanged, &neighbor.block_type)
                else {
                    continue;
                };
                if handler.target.is_none() && !above {
                    continue;
                }
                let event = Event::NeighborChanged {
                    cell,
                    changed,
                    previous: previous.clone(),
                    current: current.clone(),
                };
                context.dispatch(handler, &event).map_err(error)?;
                let after = context.block(cell).map_err(error)?;
                if neighbor != after && after.block_type == "bloxgloom:air" {
                    // Anchored removals are expanded at the next frontier,
                    // before dispatching their one lifecycle-owned callback.
                    if lifecycle.is_some()
                        && context.anchored_entity_at(cell).map_err(error)?.is_some()
                    {
                        continue;
                    }
                    if let Some(removal) =
                        catalog.gameplay_handler(EventKind::BlockRemoved, &neighbor.block_type)
                    {
                        context
                            .dispatch(
                                removal,
                                &Event::BlockRemoved {
                                    cell,
                                    previous: neighbor,
                                    cause: if above {
                                        RemovalCause::SupportLoss
                                    } else {
                                        RemovalCause::WorldEdit
                                    },
                                    random: cell_random(seed, cell, tick),
                                },
                            )
                            .map_err(error)?;
                    }
                    // The removal callback ran with its precise support-loss
                    // cause, but this transition must still notify *its* own
                    // neighbors on the next pass. Suppress only a duplicate
                    // generic removal callback, not the propagation.
                    dispatched_removals.insert(cell);
                }
            }
        }
    }
}

pub(super) fn combine_entities(
    store: &super::entities::EntityStore,
    initial: Option<super::entities::PreparedEntityTransaction>,
    mut updates: Vec<super::entities::PreparedEntityTransaction>,
) -> io::Result<Option<super::entities::PreparedEntityTransaction>> {
    if updates.is_empty() {
        return Ok(initial);
    }
    updates.extend(initial);
    store
        .combine_prepared(updates)
        .map(Some)
        .map_err(io::Error::other)
}
