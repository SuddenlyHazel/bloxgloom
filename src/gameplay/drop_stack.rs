//! Stock inventory-to-world transfer through the public inventory and drop
//! operations. Neither the client nor a render animation owns these items.
use bloxgloom_host_api::gameplay::{Context, Error, Event, Handler};

pub(crate) const KEY: &str = "bloxgloom:drop_stack";

pub(crate) struct DropStack;
impl Handler for DropStack {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested {
            position,
            arguments,
            ..
        } = event
        else {
            return Err(Error::Invalid("expected drop stack action".into()));
        };
        let [slot, low, high] = arguments.as_slice() else {
            return Err(Error::Invalid("invalid drop stack arguments".into()));
        };
        let player = context
            .player()
            .ok_or_else(|| Error::Invalid("drop stack needs a player".into()))?;
        let stack = context
            .take(
                player,
                usize::from(*slot),
                u16::from_le_bytes([*low, *high]),
            )?
            .ok_or_else(|| Error::Invalid("drop stack source is empty".into()))?;
        context.spawn_stack([position[0], position[1] + 0.8, position[2]], stack, 1_500)
    }
}
