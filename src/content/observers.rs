use super::Catalog;
use bloxgloom_host_api::{RegistrationError, gameplay::ObserverRegistration};
use std::sync::Arc;
impl Catalog {
    pub(crate) fn register_gameplay_observer(
        &mut self,
        observer: ObserverRegistration,
    ) -> Result<(), RegistrationError> {
        observer.validate()?;
        if self.gameplay_observers.len() >= 128
            || self
                .gameplay_observers
                .values()
                .any(|old| old.key == observer.key)
        {
            return Err(RegistrationError(format!(
                "{}: duplicate or over-limit observer",
                observer.key
            )));
        }
        let id = self
            .gameplay_observers
            .keys()
            .next_back()
            .map_or(0, |id| id + 1);
        self.gameplay_observers.insert(id, Arc::new(observer));
        Ok(())
    }
    pub(crate) fn gameplay_observers(&self) -> impl Iterator<Item = &Arc<ObserverRegistration>> {
        self.gameplay_observers.values()
    }
}
