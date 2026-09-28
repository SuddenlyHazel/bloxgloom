//! Builtin slot binding using the same finite-inventory operation as mods.
use bloxgloom_host_api::gameplay::{Context, Error, Event, Handler};

pub(crate) const KEY: &str = "bloxgloom:slot_move";

pub(crate) struct SlotMove;
impl Handler for SlotMove {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested { arguments, .. } = event else {
            return Err(Error::Invalid("expected slot move action".into()));
        };
        let [from, to, low, high] = arguments.as_slice() else {
            return Err(Error::Invalid("invalid slot move arguments".into()));
        };
        let player = context
            .player()
            .ok_or_else(|| Error::Invalid("slot move needs a player".into()))?;
        if context.move_slots(
            player,
            usize::from(*from),
            usize::from(*to),
            u16::from_le_bytes([*low, *high]),
        )? {
            Ok(())
        } else {
            Err(Error::Invalid("slot move does not fit".into()))
        }
    }
}
