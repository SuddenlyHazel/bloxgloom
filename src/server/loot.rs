//! Test harness for the public built-in harvest policy. Production dispatch uses
//! the shared world planning context in `server::gameplay`.
use crate::content::Catalog;
use crate::items::ItemId;
use crate::world::BlockId;
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error, Snapshot, cell_random};
use std::io;
use std::time::Duration;

pub(super) type Spawn = ([f32; 3], ItemId, u16, Duration);

#[cfg(test)]
pub(super) fn harvest(
    block: BlockId,
    position: [i32; 3],
    edit_version: u64,
    seed: u64,
) -> [Option<(ItemId, u16)>; 3] {
    let drops = harvest_with_catalog(
        crate::content::catalog(),
        block,
        position,
        edit_version,
        seed,
    )
    .unwrap();
    let mut result = [None; 3];
    assert!(drops.len() <= result.len());
    for (slot, (_, item, count, _)) in result.iter_mut().zip(drops) {
        *slot = Some((item, count));
    }
    result
}

pub(super) fn harvest_with_catalog(
    catalog: &Catalog,
    block: BlockId,
    position: [i32; 3],
    edit_version: u64,
    seed: u64,
) -> io::Result<Vec<Spawn>> {
    let block = super::gameplay::block(catalog, block).map_err(super::gameplay::error)?;
    let mut snapshot = HarvestSnapshot {
        catalog,
        block: block.clone(),
        position,
    };
    let mut context = Context::new(&mut snapshot, 256);
    crate::gameplay::harvest(
        &mut context,
        &block,
        position,
        cell_random(seed, position, edit_version),
    )
    .map_err(super::gameplay::error)?;
    let plan = context.finish().map_err(super::gameplay::error)?;
    if !plan.blocks.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "harvest adapter cannot discard staged terrain edits",
        ));
    }
    plan.drops
        .into_iter()
        .map(|drop| {
            catalog
                .items()
                .find(|item| item.key == drop.stack.item)
                .map(|item| {
                    (
                        drop.position,
                        item.id,
                        drop.stack.count,
                        Duration::from_millis(u64::from(drop.pickup_delay_ms)),
                    )
                })
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "harvest item disappeared")
                })
        })
        .collect()
}

struct HarvestSnapshot<'a> {
    catalog: &'a Catalog,
    block: Block,
    position: Cell,
}
impl Snapshot for HarvestSnapshot<'_> {
    fn seed(&self) -> u64 {
        23
    }
    fn tick(&self) -> u64 {
        0
    }
    fn project_entity_state(&self, _: u64, _: &[u8]) -> Result<Vec<u8>, Error> {
        Err(Error::Invalid(
            "entity projection unavailable in loot test snapshot".into(),
        ))
    }
    fn nearby_entities(
        &mut self,
        _: [f32; 3],
        _: f32,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Entity>, Error> {
        Err(Error::Invalid(
            "entity query unavailable in loot test snapshot".into(),
        ))
    }
    fn entity_state(&mut self, _: u64, _: &str) -> Result<Option<Vec<u8>>, Error> {
        Err(Error::Invalid(
            "entity state unavailable in loot test snapshot".into(),
        ))
    }
    fn validate_entity_state(&self, key: &str, _: &str, _: &[u8]) -> Result<(), Error> {
        Err(Error::UnknownContent(key.into()))
    }
    fn inventory_accepts(
        &self,
        _: bloxgloom_host_api::gameplay::InventoryId,
        _: usize,
        _: &bloxgloom_host_api::gameplay::Stack,
    ) -> bool {
        false
    }
    fn entity(&mut self, _: u64) -> Result<Option<bloxgloom_host_api::gameplay::Entity>, Error> {
        Err(Error::Invalid(
            "entity lookup unavailable in loot test snapshot".into(),
        ))
    }
    fn anchored_entity_at(&mut self, cell: Cell) -> Result<Option<u64>, Error> {
        Err(Error::Unavailable(cell))
    }
    fn player(&self) -> Option<u128> {
        None
    }
    fn inventory(
        &mut self,
        owner: bloxgloom_host_api::gameplay::InventoryId,
    ) -> Result<Vec<bloxgloom_host_api::gameplay::Slot>, Error> {
        Err(Error::InventoryUnavailable(owner))
    }
    fn validate_stack(&self, stack: &bloxgloom_host_api::gameplay::Stack) -> Result<(), Error> {
        super::gameplay::inventory::stack(self.catalog, stack).map(|_| ())
    }
    fn block(&mut self, cell: Cell) -> Result<Block, Error> {
        if cell == self.position {
            Ok(self.block.clone())
        } else {
            Err(Error::Unavailable(cell))
        }
    }
    fn state(&self, key: &str) -> Result<Block, Error> {
        let id = self
            .catalog
            .state_by_key(key)
            .ok_or_else(|| Error::UnknownContent(key.into()))?;
        super::gameplay::block(self.catalog, id)
    }
    fn item_exists(&self, key: &str) -> bool {
        self.catalog.items().any(|item| item.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{SAPLING, SEEDS, STICK};
    use crate::world;

    #[test]
    fn flower_harvests_itself_and_grass_and_leaves_have_distinct_loot() {
        assert_eq!(
            harvest(world::RED_FLOWER, [1, 2, 3], 1, 7),
            [Some((ItemId::new(world::RED_FLOWER.get()), 1)), None, None]
        );
        let grass: Vec<_> = (0..240)
            .flat_map(|x| harvest(world::TALL_GRASS, [x, 20, 0], 1, 7))
            .flatten()
            .collect();
        assert!(grass.len() > 40 && grass.len() < 120);
        assert!(grass.iter().all(|drop| *drop == (SEEDS, 1)));
        let leaves: Vec<_> = (0..400)
            .flat_map(|x| harvest(world::LEAVES, [x, 20, 0], 1, 7))
            .flatten()
            .collect();
        assert!(leaves.iter().any(|drop| drop.0 == SAPLING));
        assert!(leaves.iter().any(|drop| drop.0 == STICK));
        let first: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 7, 3))
            .collect();
        let repeated: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 7, 3))
            .collect();
        let next_revision: Vec<_> = (0..128)
            .map(|x| harvest(world::LEAVES, [x, 20, 0], 8, 3))
            .collect();
        assert_eq!(first, repeated);
        assert_ne!(first, next_revision);
    }
}
