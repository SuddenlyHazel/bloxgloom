//! Built-in gameplay policy. Keep this module on the public host boundary so
//! scripts and native gameplay receive the same world operations.
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error};
pub(crate) mod admin;
pub(crate) mod drop_stack;
pub(crate) mod respawn;
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
        for &(id, count) in drops {
            context.collect_drop(id, count)?;
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
        if cell[0] != changed[0]
            || cell[2] != changed[2]
            || !((previous.supports_plant && !current.supports_plant)
                || (previous.plant && previous.state.contains("[half="))
                || (current.plant && current.state.contains("[half=")))
        {
            return Ok(());
        }
        let this = context.block(*cell)?;
        if this.plant && cell[0] == changed[0] && cell[2] == changed[2] {
            if this.state.ends_with("[half=upper]")
                && cell[1].checked_sub(1) == Some(changed[1])
                && current.state != this.state.replace("[half=upper]", "[half=lower]")
            {
                context.set_block(*cell, "bloxgloom:air")?;
                return Ok(());
            }
            if this.state.ends_with("[half=lower]")
                && cell[1].checked_add(1) == Some(changed[1])
                && current.state != this.state.replace("[half=lower]", "[half=upper]")
            {
                context.set_block(*cell, "bloxgloom:air")?;
                return Ok(());
            }
        }
        if cell[0] == changed[0]
            && cell[1].checked_sub(1) == Some(changed[1])
            && cell[2] == changed[2]
            && previous.supports_plant
            && !current.supports_plant
            && this.plant
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
                | bloxgloom_host_api::gameplay::RemovalCause::Transformation
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
    // A two-cell plant owns one item; upper removal never creates a second.
    if block.plant && block.state.ends_with("[half=upper]") {
        return Ok(());
    }
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
        key if key.ends_with("_leaves") => {
            if random & 15 == 0
                && let Some(item) = &block.primary_item
            {
                drop(item)?;
            }
            if (random >> 8).is_multiple_of(5) {
                drop("bloxgloom:stick")?;
            }
            if (random >> 16).is_multiple_of(20)
                && let Some(sapling) = crate::content::jg_rtx::sapling_for_leaf(key)
            {
                drop(&sapling)?;
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
