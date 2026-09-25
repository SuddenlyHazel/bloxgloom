//! Authorization and all-or-nothing inventory planning for creative grants.

use crate::content::Catalog;
use crate::inventory::{Inventory, STACK_LIMIT};
use crate::items::ItemId;
use std::io::{self, ErrorKind};

pub(super) fn plan_grant(
    admin_profile: Option<u128>,
    profile: u128,
    inventory: &Inventory,
    item: ItemId,
    count: u16,
    catalog: &Catalog,
) -> io::Result<Option<Inventory>> {
    if admin_profile != Some(profile) || profile == 0 {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            "admin access denied",
        ));
    }
    if catalog.item(item).is_none() || !(1..=STACK_LIMIT).contains(&count) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid admin grant",
        ));
    }
    let mut next = inventory.clone();
    if next.insert_with_catalog(item, count, catalog) != 0 {
        return Ok(None);
    }
    Ok(Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_rejects_unauthorized_and_never_partially_fills() {
        let catalog = Catalog::builtins();
        let item = catalog.items().next().unwrap().id;
        let original = Inventory::default();
        assert_eq!(
            plan_grant(None, 17, &original, item, 128, &catalog)
                .unwrap_err()
                .kind(),
            ErrorKind::PermissionDenied
        );
        let granted = plan_grant(Some(17), 17, &original, item, 128, &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(granted.slots[0].as_ref().unwrap().count, 128);
        assert_eq!(original.slots[0], None);
        let mut full = granted;
        for slot in &mut full.slots[1..] {
            *slot = Some(crate::inventory::Stack::new(item, 128));
        }
        assert!(
            plan_grant(Some(17), 17, &full, item, 1, &catalog)
                .unwrap()
                .is_none()
        );
        assert_eq!(full.revision, 1);
    }
}
