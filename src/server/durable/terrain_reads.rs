//! Exact read dependencies for gameplay planners: terrain, clock, entities and
//! profile cells. Admission reserves these keys until confirmed apply, including
//! empty terrain and missing profile cells, while a receipt is outstanding.
use super::*;
use crate::world::{ChunkReadStamp, World};
type PlayerPoses = BTreeMap<u64, ([f32; 3], u64, bool)>;
#[derive(Clone, Debug, Default)]
pub(in crate::server) struct TerrainReads {
    pub clock: Option<crate::server::world_time::ReadStamp>,
    profiles: BTreeMap<(crate::server::registry::SystemId, u128), Option<u64>>,
    inventories: BTreeMap<u128, u64>,
    terrain: BTreeMap<ChunkKey, ChunkReadStamp>,
    entities: super::super::entities::EntityDependencies,
    /// Transient collider poses are rechecked before WAL admission. After
    /// admission the historical collision decision is fixed; later player
    /// movement does not invalidate an already accepted durable decision.
    players: Option<PlayerPoses>,
}
impl TerrainReads {
    pub fn players(&mut self, state: &crate::server::State) -> io::Result<()> {
        if state.clients.len() > 256 {
            return Err(io::Error::new(
                ErrorKind::QuotaExceeded,
                "moving player capture capacity",
            ));
        }
        let poses = state
            .clients
            .iter()
            .map(|(&session, client)| {
                (
                    session,
                    (
                        client.position(),
                        client.movement.last_seq(),
                        client.movement.crouching(),
                    ),
                )
            })
            .collect();
        if self.players.as_ref().is_some_and(|old| old != &poses) {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "moving player capture changed",
            ));
        }
        self.players = Some(poses);
        Ok(())
    }
    pub fn players_current(&self, state: &crate::server::State) -> bool {
        self.players.as_ref().is_none_or(|poses| {
            poses.len() == state.clients.len()
                && poses.iter().all(|(session, expected)| {
                    state.clients.get(session).is_some_and(|client| {
                        (
                            client.position(),
                            client.movement.last_seq(),
                            client.movement.crouching(),
                        ) == *expected
                    })
                })
        })
    }
    pub fn inventory(&mut self, profile: u128, revision: u64) -> io::Result<()> {
        if let Some(old) = self.inventories.get(&profile) {
            if *old != revision {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "inventory read changed during planning",
                ));
            }
        } else {
            if self.inventories.len() >= 8 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "profile inventory read limit exceeded",
                ));
            }
            self.inventories.insert(profile, revision);
        }
        Ok(())
    }
    pub fn inventories_current(&self, state: &crate::server::State) -> bool {
        self.inventories.iter().all(|(profile, revision)| {
            state
                .clients
                .values()
                .find(|c| c.profile == *profile)
                .map(|c| c.inventory.revision)
                .or_else(|| {
                    state
                        .durability
                        .inventory_overlay
                        .get(profile)
                        .map(|i| i.revision)
                })
                .or_else(|| state.durability.inventory_revisions.get(profile).copied())
                .is_none_or(|current| current == *revision)
        })
    }
    pub fn profile(
        &mut self,
        system: &crate::server::registry::SystemId,
        profile: u128,
        revision: Option<u64>,
    ) -> io::Result<()> {
        let key = (system.clone(), profile);
        if let Some(old) = self.profiles.get(&key) {
            if *old != revision {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "profile read changed during planning",
                ));
            }
        } else {
            if self.profiles.len() >= 64 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "profile read budget exceeded",
                ));
            }
            self.profiles.insert(key, revision);
        }
        Ok(())
    }
    pub fn profiles_current(
        &self,
        runtime: &crate::server::runtime::systems::SystemRuntime,
    ) -> bool {
        self.profiles.iter().all(|((system, profile), revision)| {
            runtime
                .owner_snapshot(system, crate::server::parallel::OwnerKey::Profile(*profile))
                .map(|(rev, _)| rev)
                == *revision
        })
    }
    pub fn entities(
        &mut self,
        dependencies: super::super::entities::EntityDependencies,
    ) -> io::Result<()> {
        self.entities
            .merge(dependencies)
            .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))
    }
    pub fn entities_current(&self, store: &super::super::entities::EntityStore) -> bool {
        self.entities.is_current(store)
    }
    pub fn read(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
    ) -> io::Result<Option<crate::world::BlockId>> {
        let Some(block) = world.cached_block(x, y, z) else {
            return Ok(None);
        };
        let key = crate::world::world_to_chunk(x, y, z).0;
        if let Some(stamp) = self.terrain.get(&key) {
            if !stamp.is_current() {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "terrain read changed during planning",
                ));
            }
        } else {
            if self.terrain.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.terrain.insert(
                key,
                world
                    .cached_read_stamp(key)
                    .expect("resident read has authority stamp"),
            );
        }
        Ok(Some(block))
    }
    pub fn extend(&mut self, other: Self) -> io::Result<()> {
        if !self.is_current() || !other.is_current() {
            return Err(io::Error::new(ErrorKind::WouldBlock, "stale terrain read"));
        }
        self.entities(other.entities)?;
        if let Some(players) = other.players {
            if self.players.as_ref().is_some_and(|old| old != &players) {
                return Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "moving player capture changed",
                ));
            }
            self.players = Some(players);
        }
        for (profile, revision) in other.inventories {
            self.inventory(profile, revision)?;
        }
        for ((system, profile), revision) in other.profiles {
            self.profile(&system, profile, revision)?;
        }
        if self.clock.is_none() {
            self.clock = other.clock;
        }
        for (key, stamp) in other.terrain {
            if !self.terrain.contains_key(&key) && self.terrain.len() >= 256 {
                return Err(io::Error::new(
                    ErrorKind::QuotaExceeded,
                    "terrain read budget exceeded",
                ));
            }
            self.terrain.entry(key).or_insert(stamp);
        }
        Ok(())
    }
    pub fn is_current(&self) -> bool {
        self.terrain.values().all(ChunkReadStamp::is_current)
            && self
                .clock
                .as_ref()
                .is_none_or(crate::server::world_time::ReadStamp::is_current)
    }
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty()
            && self.entities.is_empty()
            && self.clock.is_none()
            && self.profiles.is_empty()
            && self.inventories.is_empty()
            && self.players.is_none()
    }
    pub fn keys(&self) -> impl Iterator<Item = StateKey> + '_ {
        self.terrain
            .keys()
            .copied()
            .map(chunk_state_key)
            .chain(self.entities.keys())
            .chain(
                self.inventories
                    .keys()
                    .copied()
                    .map(super::inventory_state_key),
            )
            .chain(self.profiles.keys().map(|(system, profile)| {
                crate::server::runtime::owner_codec::owner_state_key(
                    system,
                    crate::server::parallel::OwnerKey::Profile(*profile),
                )
            }))
            .chain(
                self.clock
                    .iter()
                    .map(|_| crate::server::world_time::state_key()),
            )
    }
}
