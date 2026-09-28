//! Built-in gameplay policy. Keep this module on the public host boundary so
//! scripts and native gameplay receive the same world operations.
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error, InventoryId};
pub(crate) mod admin;
pub(crate) mod drop_stack;
pub(crate) mod slot_move;

pub(crate) struct Pickup;
impl bloxgloom_host_api::gameplay::Handler for Pickup {
    fn handle(
        &self,
        context: &mut Context<'_>,
        event: &bloxgloom_host_api::gameplay::Event,
    ) -> Result<(), Error> {
        let bloxgloom_host_api::gameplay::Event::PickupRequested { drops, .. } = event else {
            return Ok(());
        };
        let Some(player) = context.player() else {
            return Err(Error::Invalid("pickup needs a player".into()));
        };
        for &(id, count) in drops {
            let source = InventoryId::Entity(id);
            let Some(slot) = context.inventory(source)?.into_iter().next() else {
                continue;
            };
            let Some(stack) = slot.stack else {
                continue;
            };
            if !slot.extract {
                continue;
            }
            let mut remaining = count.min(stack.count);
            for (index, destination) in context.inventory(player)?.into_iter().enumerate() {
                if remaining == 0 {
                    break;
                }
                if !destination.insert {
                    continue;
                }
                let capacity = match destination.stack {
                    Some(other)
                        if other.item == stack.item && other.components == stack.components =>
                    {
                        128 - other.count
                    }
                    None => 128,
                    _ => 0,
                };
                let amount = remaining.min(capacity);
                if amount != 0 && context.transfer(source, 0, player, index, amount)? {
                    remaining -= amount;
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct Harvest;
pub(crate) struct PlantSupport;
impl bloxgloom_host_api::gameplay::Handler for PlantSupport {
    fn handle(
        &self,
        context: &mut Context<'_>,
        event: &bloxgloom_host_api::gameplay::Event,
    ) -> Result<(), Error> {
        let bloxgloom_host_api::gameplay::Event::NeighborChanged {
            cell,
            changed,
            previous,
            current,
        } = event
        else {
            return Ok(());
        };
        if cell[0] == changed[0]
            && cell[1].checked_sub(1) == Some(changed[1])
            && cell[2] == changed[2]
            && previous.supports_plant
            && !current.supports_plant
            && context.block(*cell)?.plant
        {
            context.set_block(*cell, "bloxgloom:air")?;
        }
        Ok(())
    }
}
impl bloxgloom_host_api::gameplay::Handler for Harvest {
    fn handle(
        &self,
        context: &mut Context<'_>,
        event: &bloxgloom_host_api::gameplay::Event,
    ) -> Result<(), Error> {
        let bloxgloom_host_api::gameplay::Event::BlockRemoved {
            cell,
            previous,
            random,
            cause,
            ..
        } = event
        else {
            return Ok(());
        };
        if matches!(
            cause,
            bloxgloom_host_api::gameplay::RemovalCause::AnchoredBreak
                | bloxgloom_host_api::gameplay::RemovalCause::Burn
        ) {
            return Ok(());
        }
        harvest(context, previous, *cell, *random)
    }
}

pub(crate) fn harvest(
    context: &mut Context<'_>,
    block: &Block,
    cell: Cell,
    random: u64,
) -> Result<(), Error> {
    let position = cell.map(|n| n as f32 + 0.5);
    let mut drop = |item: &str| context.spawn_drop(position, item, 1, 250);
    match block.block_type.as_str() {
        "bloxgloom:air" => {}
        "bloxgloom:tall_grass" => {
            if random.is_multiple_of(3) {
                drop("bloxgloom:seeds")?;
            }
        }
        "bloxgloom:leaves" => {
            if random & 15 == 0 {
                drop("bloxgloom:leaves")?;
            }
            if (random >> 8).is_multiple_of(5) {
                drop("bloxgloom:stick")?;
            }
            if (random >> 16).is_multiple_of(20) {
                drop("bloxgloom:sapling")?;
            }
        }
        _ => {
            if let Some(item) = &block.primary_item {
                drop(item)?;
            }
        }
    }
    Ok(())
}
