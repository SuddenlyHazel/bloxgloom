//! Authoritative membership transitions, staged through lifecycle WAL callbacks.
use super::{
    State, capture,
    lifecycle::{QUEUE_LIMIT, enqueue},
};
use bloxgloom_host_api::{
    gameplay::Player,
    players::{Event, EventKind},
};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Default)]
pub(in crate::server) struct Runtime {
    /// Retain departed snapshots until all leaves fit the callback queue.
    members: BTreeMap<(u128, u64, String), Player>,
    transition: u64,
}
/// Reconcile after authoritative movement, teleport, admission and departure.
/// Queue pressure incrementally converges membership; endpoint changes that occur
/// entirely while overloaded may coalesce until capacity returns.
pub(in crate::server) fn reconcile(state: &mut State) {
    let players = capture(state)
        .into_iter()
        .filter(|p| state.player_runtime.is_admitted(p.profile, p.session))
        .collect::<Vec<_>>();
    let mut desired = BTreeMap::new();
    for region in state.world.catalog().regions() {
        for player in &players {
            if region.contains(player.position) {
                desired.insert(
                    (player.profile, player.session, region.key.clone()),
                    player.clone(),
                );
            }
        }
    }
    let mut transitions = Vec::new();
    for (key, player) in &state.region_runtime.members {
        if !desired.contains_key(key) {
            transitions.push((key.clone(), player.clone(), EventKind::RegionLeft));
        }
    }
    for (key, player) in &desired {
        if !state.region_runtime.members.contains_key(key) {
            transitions.push((key.clone(), player.clone(), EventKind::RegionEntered));
        }
    }
    let available = QUEUE_LIMIT.saturating_sub(state.player_runtime.queue.len());
    for ((profile, session, region_key), player, kind) in transitions.into_iter().take(available) {
        let Some(region) = state
            .world
            .catalog()
            .regions()
            .find(|r| r.key == region_key)
        else {
            continue;
        };
        let Some(reg) = state
            .world
            .catalog()
            .player_lifecycles()
            .find(|r| r.key == region.service)
            .cloned()
        else {
            continue;
        };
        let Some(transition) = state.region_runtime.transition.checked_add(1) else {
            return;
        };
        state.region_runtime.transition = transition;
        let final_session = (kind == EventKind::RegionLeft).then(|| {
            state
                .player_runtime
                .sessions
                .get(&(reg.key.clone(), profile, session))
                .map(|s| s.data.clone())
                .unwrap_or_default()
        });
        enqueue(
            &mut state.player_runtime,
            Arc::clone(&reg),
            Event {
                kind,
                profile,
                transition,
                player: Some(player.clone()),
                region: Some(region_key.clone()),
            },
            final_session,
        );
        let key = (profile, session, region_key);
        if kind == EventKind::RegionLeft {
            state.region_runtime.members.remove(&key);
        } else {
            state.region_runtime.members.insert(key, player);
        }
    }
    for (key, player) in desired {
        if let Some(previous) = state.region_runtime.members.get_mut(&key) {
            *previous = player;
        }
    }
}
