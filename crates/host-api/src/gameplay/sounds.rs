use super::{Context, Error};
use crate::sound::{Event, Kind};
impl Context<'_> {
    pub fn sound(&mut self, voice: String, mut kind: Kind) -> Result<(), Error> {
        self.charge()?;
        let Some(owner) = self.handler_namespace.clone() else {
            return self.fail(Error::Invalid("sound requires a handler owner".into()));
        };
        if let Kind::Play {
            clip,
            position,
            entity,
            ..
        } = &mut kind
        {
            if !self.snapshot.sound_registered(clip) {
                return self.fail(Error::Invalid(format!("unregistered sound {clip}")));
            }
            if let Some(id) = entity {
                match self.snapshot.entity(*id) {
                    Ok(Some(value)) => *position = value.position,
                    Ok(None) => {
                        return self.fail(Error::Invalid("sound entity unavailable".into()));
                    }
                    Err(error) => return self.fail(error),
                }
            }
        }
        let event = Event { owner, voice, kind };
        if !event.validate() || self.plan.sounds.len() >= 16 {
            return self.fail(Error::Invalid(
                "invalid sound or sound events/transaction exceeds 16".into(),
            ));
        }
        self.plan.sounds.push(event);
        Ok(())
    }
}
