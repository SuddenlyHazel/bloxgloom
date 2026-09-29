//! Deeply immutable delivery values; identities never pass through f64/u64 casts.
use super::*;

pub(super) fn inbox(lua: &Lua, deliveries: &[api::IntentDelivery]) -> mlua::Result<Table> {
    if deliveries.len() > api::MAX_INTENTS_PER_JOB
        || deliveries
            .iter()
            .any(|d| d.payload.len() > api::MAX_INTENT_PAYLOAD_BYTES)
    {
        return Err(mlua::Error::RuntimeError(
            "system inbox exceeds bound".into(),
        ));
    }
    let inbox = lua.create_table()?;
    for (index, delivery) in deliveries.iter().enumerate() {
        let api::Owner::Chunk(source) = delivery.id.source else {
            return Err(mlua::Error::RuntimeError(
                "expected chunk intent source".into(),
            ));
        };
        let source = lua.create_sequence_from(source)?;
        source.set_readonly(true);
        let id = lua.create_table()?;
        id.set("source", source)?;
        id.set(
            "revision",
            crate::server::script::handles::revision(lua, delivery.id.revision)?,
        )?;
        id.set("revision_lo", delivery.id.revision as u32)?;
        id.set("revision_hi", (delivery.id.revision >> 32) as u32)?;
        id.set("ordinal", delivery.id.ordinal)?;
        id.set_readonly(true);
        let message = lua.create_table()?;
        message.set("id", id)?;
        message.set(
            "produced_tick",
            crate::server::script::handles::tick(lua, delivery.produced_tick)?,
        )?;
        message.set("produced_tick_lo", delivery.produced_tick as u32)?;
        message.set("produced_tick_hi", (delivery.produced_tick >> 32) as u32)?;
        message.set("payload", lua.create_string(&delivery.payload)?)?;
        message.set_readonly(true);
        inbox.raw_set(index + 1, message)?;
    }
    inbox.set_readonly(true);
    Ok(inbox)
}
