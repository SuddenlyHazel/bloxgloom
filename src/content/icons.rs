use super::*;
use bloxgloom_host_api::{RegistrationError as ApiError, icon::ItemIcon};
mod builtins;
pub(crate) mod script;
impl Catalog {
    pub(crate) fn register_item_icon(&mut self, mut icon: ItemIcon) -> Result<(), ApiError> {
        icon.validate()?;
        if !self.item_keys.contains(&icon.item) || self.item_icons.contains_key(&icon.item) {
            return Err(ApiError("missing item or duplicate item icon".into()));
        }
        icon.palette.sort_by_key(|(symbol, _)| *symbol);
        self.item_icons
            .insert(icon.item.clone(), std::sync::Arc::new(icon));
        Ok(())
    }
    pub(crate) fn item_icon(&self, item: ItemId) -> Option<&bloxgloom_host_api::icon::ItemIcon> {
        self.item_icons
            .get(self.item(item)?.key.as_ref())
            .map(|icon| icon.as_ref())
    }
    pub(super) fn builtin_item_icons(&mut self) {
        for icon in builtins::definitions() {
            self.register_item_icon(icon).expect("builtin item icon");
        }
    }
}
