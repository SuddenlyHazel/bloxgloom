//! Native respawn shares registered semantic action admission and WAL receipts.
use bloxgloom_host_api::gameplay::{Context, Error, Event, Handler};
pub(crate) const KEY: &str = "bloxgloom:respawn";
pub(crate) struct Respawn;
impl Handler for Respawn {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested { arguments, .. } = event else {
            return Err(Error::Invalid("expected respawn request".into()));
        };
        let bytes: [u8; 8] = arguments
            .as_slice()
            .try_into()
            .map_err(|_| Error::Invalid("invalid respawn arguments".into()))?;
        context.native_respawn_at_spawn(u64::from_le_bytes(bytes))
    }
}
