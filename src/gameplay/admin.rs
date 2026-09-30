//! Builtin console actions use the same registered decision context as mods.
use bloxgloom_host_api::gameplay::{Context, Error, Event, Handler};

pub(crate) const GIVE: &str = "bloxgloom:admin_give";
pub(crate) const SPAWN: &str = "bloxgloom:admin_spawn";
pub(crate) const TIME: &str = "bloxgloom:admin_time";

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
            TIME => {
                let bytes: [u8; 8] = arguments
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Invalid("invalid world time request".into()))?;
                context.admin_set_time(u64::from_le_bytes(bytes))
            }
            GIVE => {
                let (&length, rest) = arguments
                    .split_first()
                    .ok_or_else(|| Error::Invalid("invalid grant".into()))?;
                let (key, count) = rest
                    .split_at_checked(usize::from(length))
                    .ok_or_else(|| Error::Invalid("invalid grant".into()))?;
                let [count] = count else {
                    return Err(Error::Invalid("invalid grant count".into()));
                };
                let key = std::str::from_utf8(key)
                    .map_err(|_| Error::Invalid("invalid item key".into()))?;
                if !context.admin_give(key, u16::from(*count))? {
                    return Err(Error::Invalid("grant does not fit".into()));
                }
                Ok(())
            }
            SPAWN => {
                let (&length, bytes) = arguments
                    .split_first()
                    .ok_or_else(|| Error::Invalid("invalid creature key".into()))?;
                if bytes.len() != usize::from(length) {
                    return Err(Error::Invalid("invalid creature key".into()));
                }
                let key = std::str::from_utf8(bytes)
                    .map_err(|_| Error::Invalid("invalid creature key".into()))?;
                context.admin_spawn(key)
            }
            _ => Err(Error::Invalid("unknown admin action".into())),
        }
    }
}
