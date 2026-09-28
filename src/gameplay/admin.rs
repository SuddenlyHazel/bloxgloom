//! Builtin console actions use the same registered decision context as mods.
use bloxgloom_host_api::gameplay::{Context, Error, Event, Handler};

pub(crate) const GIVE: &str = "bloxgloom:admin_give";
pub(crate) const SPAWN: &str = "bloxgloom:admin_spawn";

pub(crate) struct Admin;
impl Handler for Admin {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::ActionRequested {
            action, arguments, ..
        } = event
        else {
            return Err(Error::Invalid("expected admin action".into()));
        };
        match action.as_str() {
            GIVE => {
                let (count, key) = arguments
                    .split_at_checked(2)
                    .ok_or_else(|| Error::Invalid("invalid grant".into()))?;
                let key = std::str::from_utf8(key)
                    .map_err(|_| Error::Invalid("invalid item key".into()))?;
                if !context.admin_give(key, u16::from_le_bytes([count[0], count[1]]))? {
                    return Err(Error::Invalid("grant does not fit".into()));
                }
                Ok(())
            }
            SPAWN => {
                let key = std::str::from_utf8(arguments)
                    .map_err(|_| Error::Invalid("invalid creature key".into()))?;
                context.admin_spawn(key)
            }
            _ => Err(Error::Invalid("unknown admin action".into())),
        }
    }
}
