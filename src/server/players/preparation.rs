//! Atomic profile state/reward and receipt-bound session output preparation.
use super::callbacks::{invoke, profile_state};
use super::{
    State,
    lifecycle::{Published, key},
};
use crate::{
    inventory::InventoryStore,
    server::{durable::CommitAction, registry::SystemId, simulation::TickId},
};
use bloxgloom_host_api::players::{Event, EventKind, Registration};
use std::io;
fn deadline(old: Option<u64>, delay: Option<Option<u32>>, tick: u64) -> io::Result<Option<u64>> {
    match delay {
        None => Ok(old),
        Some(None) => Ok(None),
        Some(Some(delay)) if (1..=100_000).contains(&delay) => tick
            .checked_add(u64::from(delay))
            .map(Some)
            .ok_or_else(|| io::Error::other("player deadline exhausted")),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid player delay",
        )),
    }
}
pub(super) fn prepare(
    state: &mut State,
    reg: &Registration,
    event: &Event,
    tick: TickId,
    final_session: Option<&[u8]>,
) -> io::Result<CommitAction> {
    let client = state
        .clients
        .iter()
        .find(|(_, c)| c.profile == event.profile);
    let client_id = client.map(|(id, _)| *id);
    let before = match client {
        Some((_, c)) => c.inventory.clone(),
        None => state
            .durability
            .inventory_overlay
            .get(&event.profile)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| state.inventory_store.load(event.profile))?,
    };
    let (decision, inventory, operations, reads) =
        invoke(state, reg, event, &before, final_session)?;
    if decision.spawn.is_some() || decision.deny.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "spawn and deny are PlayerJoining decisions",
        ));
    }
    let value = decision
        .state
        .unwrap_or(profile_state(state, reg, event.profile)?);
    let system = SystemId::new(&reg.key).map_err(|_| io::Error::other("invalid player service"))?;
    let old = if event.kind == EventKind::ProfileTick {
        None
    } else {
        state
            .system_runtime
            .profile_deadline(&system, event.profile)
    };
    let due = deadline(old, decision.profile_delay, tick.get())?;
    if state
        .system_runtime
        .owner_snapshot(
            &system,
            crate::server::parallel::OwnerKey::Profile(event.profile),
        )
        .is_none()
        && !state
            .system_runtime
            .has_profile_insert_room(state.durability.pending_profile_inserts())
    {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "profile cell capacity exhausted",
        ));
    }
    if decision
        .session_data
        .as_ref()
        .is_some_and(|data| data.len() > 4096)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "session state byte limit exceeded",
        ));
    }
    let changes = super::state::prepare(&state.system_runtime, reg, event.profile, value, due)?;
    let session = event
        .player
        .as_ref()
        .map(|p| {
            let key = (reg.key.clone(), p.profile, p.session);
            let old = state.player_runtime.sessions.get(&key);
            let data = decision
                .session_data
                .unwrap_or_else(|| old.map(|s| s.data.clone()).unwrap_or_default());
            let old_due = if event.kind == EventKind::SessionTick {
                None
            } else {
                old.and_then(|s| s.deadline)
            };
            Ok::<_, io::Error>((
                key,
                data,
                deadline(old_due, decision.session_delay, tick.get())?,
            ))
        })
        .transpose()?;
    Ok(CommitAction {
        client_id,
        profile: Some(event.profile),
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: inventory
            .as_ref()
            .map(|_| InventoryStore::encode_snapshot_with_catalog(&before, state.world.catalog()))
            .transpose()?,
        inventory,
        world_edits: vec![],
        terrain_reads: reads,
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        entities: None,
        entity_wakes: vec![],
        owner_changes: changes,
        player_publication: Some(Published::Lifecycle {
            key: key(reg, event),
            session,
            operations,
        }),
    })
}
