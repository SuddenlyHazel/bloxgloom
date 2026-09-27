use crate::{content::Catalog, inventory::Inventory};
use bloxgloom_host_api::gameplay::{Components, Error, Slot, Stack};

pub(super) fn capture(catalog: &Catalog, inventory: &Inventory) -> Result<Vec<Slot>, Error> {
    inventory
        .slots
        .iter()
        .map(|slot| {
            let stack = slot
                .as_ref()
                .map(|stack| {
                    let item = catalog
                        .item(stack.item)
                        .ok_or_else(|| Error::Host("unknown inventory item".into()))?;
                    Ok(Stack {
                        item: item.key.to_string(),
                        count: stack.count,
                        components: stack.components.as_ref().map(|p| Components {
                            version: p.version,
                            bytes: p.bytes.to_vec(),
                        }),
                    })
                })
                .transpose()?;
            Ok(Slot {
                stack,
                insert: true,
                extract: true,
            })
        })
        .collect()
}

pub(in crate::server) fn stack(
    catalog: &Catalog,
    value: &Stack,
) -> Result<crate::inventory::Stack, Error> {
    let id = catalog
        .item_by_key(&value.item)
        .ok_or_else(|| Error::UnknownContent(value.item.clone()))?;
    let stack = match &value.components {
        Some(payload) => crate::inventory::Stack::with_components(
            id,
            value.count,
            payload.version,
            payload.bytes.clone(),
        )
        .ok_or_else(|| Error::Invalid("invalid item components".into()))?,
        None => crate::inventory::Stack::new(id, value.count),
    };
    if !stack.valid_in(catalog) {
        return Err(Error::Invalid(format!(
            "invalid stack/schema: {}",
            value.item
        )));
    }
    Ok(stack)
}

pub(super) fn apply(
    catalog: &Catalog,
    before: &Inventory,
    slots: Vec<Option<Stack>>,
) -> Result<Inventory, Error> {
    if slots.len() != before.slots.len() {
        return Err(Error::Invalid("inventory shape changed".into()));
    }
    let mut after = before.clone();
    for (destination, source) in after.slots.iter_mut().zip(slots) {
        *destination = source
            .as_ref()
            .map(|value| stack(catalog, value))
            .transpose()?;
    }
    if after.slots != before.slots {
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("inventory revision exhausted".into()))?;
    }
    Ok(after)
}
