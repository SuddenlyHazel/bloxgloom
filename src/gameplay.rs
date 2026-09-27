//! Built-in gameplay policy. Keep this module on the public host boundary so
//! scripts and native gameplay receive the same world operations.
use bloxgloom_host_api::gameplay::{Block, Cell, Context, Error};

pub(crate) struct Harvest;
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
        if *cause == bloxgloom_host_api::gameplay::RemovalCause::AnchoredBreak {
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
