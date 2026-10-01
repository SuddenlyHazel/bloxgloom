//! V44 carries closed anchored declarations around the existing client artifact.
//! Callback identities are metadata; server source and execution never travel.
use super::*;
use bloxgloom_host_api::{anchored::AnchoredBlockEntity, lifecycle::FootprintCell};
use std::sync::Arc;

pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x2c";

pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.anchored.is_empty() {
        return Ok(bundle);
    }
    if declarations.anchored.len() > MAX_PACKAGES * 8 {
        return Err(invalid());
    }
    let mut own = declarations.anchored.iter().collect::<Vec<_>>();
    own.sort_by(|a, b| a.entity.cmp(&b.entity));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(own.len())?;
    for d in own {
        encode(&mut writer, d)?;
    }
    let key = CacheKey(Sha256::digest(&writer.0).into());
    ClientBundle::decode_verify(&writer.0, key)
}

pub(in crate::server::script::package::client) fn decode(
    bytes: &[u8],
    expected: CacheKey,
) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.starts_with(MAGIC) {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(MAX_PACKAGES * 8)?;
    if count == 0 || !startup.anchored.is_empty() {
        return Err(invalid());
    }
    let mut per_package = BTreeMap::<String, usize>::new();
    let mut previous = String::new();
    let builtins = crate::content::Catalog::builtins();
    for _ in 0..count {
        let entity = reader.text(129)?;
        let (owner, local) = entity.split_once(':').ok_or_else(invalid)?;
        if entity <= previous || !identifier(local) {
            return Err(invalid());
        }
        let package = startup
            .packages
            .iter()
            .find(|p| p.key == format!("{owner}:package"))
            .ok_or_else(invalid)?;
        if ![composition::CONTENT, composition::ANCHORED_ENTITIES]
            .iter()
            .all(|capability| package.requires.iter().any(|r| r == capability))
        {
            return Err(invalid());
        }
        let total = per_package.entry(owner.into()).or_default();
        *total += 1;
        if *total > 8 {
            return Err(invalid());
        }
        let block = reader.text(129)?;
        let placement_item = reader.text(129)?;
        let anchor_state = reader.text(1024)?;
        if block
            .split_once(':')
            .is_none_or(|(namespace, key)| namespace != owner || !identifier(key))
            || placement_item
                .split_once(':')
                .is_none_or(|(namespace, key)| {
                    (namespace != owner && namespace != "bloxgloom") || !identifier(key)
                })
            || startup.anchored.iter().any(|d| d.block == block)
            || startup
                .storage
                .iter()
                .any(|d| d.storage.block == block || d.storage.entity == entity)
            || startup
                .machines
                .iter()
                .any(|d| d.machine.block == block || d.machine.entity == entity)
        {
            return Err(invalid());
        }
        let placement_cost = word(&mut reader)?;
        let removal_refund = word(&mut reader)?;
        let schema_version = word(&mut reader)?;
        let schema_fingerprint =
            u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        let max_state_bytes = reader.count(65_536)?;
        let max_public_bytes = reader.count(4096)?;
        let interval = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
        let mut footprint = Vec::new();
        for _ in 0..reader.count(64)? {
            let offset = axes(&mut reader)?;
            let state = reader.text(1024)?;
            footprint.push(FootprintCell { offset, state });
        }
        let mut observe = Vec::new();
        for _ in 0..reader.count(64)? {
            observe.push(axes(&mut reader)?);
        }
        let interaction = reader.field(239)?.to_vec();
        let d = AnchoredBlockEntity {
            entity: entity.clone(),
            block,
            placement_item,
            anchor_state,
            footprint,
            placement_cost,
            removal_refund,
            schema_version,
            schema_fingerprint,
            max_state_bytes,
            max_public_bytes,
            interval,
            observe,
            interaction,
            behavior: Arc::new(crate::server::script::anchored::ScriptAnchored::client()),
        };
        d.validate().map_err(|_| invalid())?;
        let own_block = startup
            .blocks
            .iter()
            .find(|block| block.key == d.block)
            .ok_or_else(invalid)?;
        let valid_state = |state: &str| crate::server::script::startup::has_state(own_block, state);
        let valid_item = startup.items.iter().any(|item| {
            item.key == d.placement_item
                && item.placeable.as_deref() == Some(d.anchor_state.as_str())
        }) || builtins.items().any(|item| {
            item.key == d.placement_item
                && item.placeable == builtins.state_by_key(&d.anchor_state)
                && item.placeable.is_some()
        });
        if !valid_state(&d.anchor_state)
            || d.footprint.iter().any(|cell| !valid_state(&cell.state))
            || !valid_item
        {
            return Err(invalid());
        }
        previous = entity;
        startup.anchored.push(d);
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    // Retire the inner canonical buffer before allocating the outer one.
    // Nested compatibility wrappers must not add another full payload copy
    // to the download + canonical + decoded-payload reservation.
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}

fn word(reader: &mut Reader<'_>) -> Result<u16, ScriptError> {
    Ok(u16::from_le_bytes(
        reader.field(2)?.try_into().map_err(|_| invalid())?,
    ))
}

fn axes(reader: &mut Reader<'_>) -> Result<[i32; 3], ScriptError> {
    let values: [u8; 3] = reader.field(3)?.try_into().map_err(|_| invalid())?;
    Ok(values.map(|value| i32::from(value as i8)))
}

fn encode(writer: &mut Writer, d: &AnchoredBlockEntity) -> Result<(), ScriptError> {
    d.validate().map_err(|_| invalid())?;
    for key in [&d.entity, &d.block, &d.placement_item, &d.anchor_state] {
        writer.field(key.as_bytes())?;
    }
    writer.field(&d.placement_cost.to_le_bytes())?;
    writer.field(&d.removal_refund.to_le_bytes())?;
    writer.field(&d.schema_version.to_le_bytes())?;
    writer.field(&d.schema_fingerprint.to_le_bytes())?;
    writer.count(d.max_state_bytes)?;
    writer.count(d.max_public_bytes)?;
    writer.field(&d.interval.to_le_bytes())?;
    writer.count(d.footprint.len())?;
    for cell in &d.footprint {
        writer.field(&cell.offset.map(|axis| axis as i8 as u8))?;
        writer.field(cell.state.as_bytes())?;
    }
    writer.count(d.observe.len())?;
    for offset in &d.observe {
        writer.field(&offset.map(|axis| axis as i8 as u8))?;
    }
    writer.field(&d.interaction)?;
    Ok(())
}

#[cfg(test)]
mod tests;
