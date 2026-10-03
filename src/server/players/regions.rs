//! Authoritative membership transitions, staged through lifecycle WAL callbacks.
use super::{
    State, capture,
    lifecycle::{QUEUE_LIMIT, enqueue},
};
use bloxgloom_host_api::{
    gameplay::Player,
    players::{Event, EventKind},
    regions::Registration,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const WORDS: usize = bloxgloom_host_api::regions::MAX_REGIONS / 64;
type Membership = [u64; WORDS];
type SessionKey = (u128, u64);
struct Member {
    /// All overlapping regions share one captured model/name snapshot.
    player: Arc<Player>,
    inside: Membership,
}
#[derive(Default)]
pub(in crate::server) struct Runtime {
    /// Departed snapshots remain until their leave transitions fit the queue.
    members: BTreeMap<SessionKey, Member>,
    transition: u64,
}
fn membership(regions: &[&Registration], player: Option<&Player>) -> Membership {
    let mut result = [0; WORDS];
    if let Some(player) = player {
        for (index, region) in regions.iter().enumerate() {
            if region.contains(player.position) {
                result[index / 64] |= 1 << (index % 64);
            }
        }
    }
    result
}
fn contains(bits: &Membership, index: usize) -> bool {
    bits[index / 64] & (1 << (index % 64)) != 0
}
/// Reconcile accepted endpoints. Leaves precede enters for each exact session.
/// Pressure incrementally converges membership; changes occurring entirely while
/// overloaded may coalesce. Steady ticks retain only one snapshot per player.
pub(in crate::server) fn reconcile(state: &mut State) {
    let catalog = state.world.catalog_arc();
    let regions = catalog.regions().collect::<Vec<_>>();
    if regions.is_empty() {
        state.region_runtime.members.clear();
        return;
    }
    let captured = capture(state)
        .into_iter()
        .map(|player| ((player.profile, player.session), Arc::new(player)))
        .collect::<BTreeMap<_, _>>();
    let sessions = state
        .region_runtime
        .members
        .keys()
        .chain(captured.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut available = QUEUE_LIMIT.saturating_sub(state.player_runtime.queue.len());
    for (profile, session) in sessions {
        let admitted = state.player_runtime.is_admitted(profile, session);
        let previous = state.region_runtime.members.get(&(profile, session));
        if previous.is_none() && (!admitted || state.region_runtime.members.len() >= QUEUE_LIMIT) {
            continue;
        }
        let Some(player) = captured
            .get(&(profile, session))
            .or_else(|| previous.map(|member| &member.player))
            .cloned()
        else {
            continue;
        };
        let mut inside = previous.map_or([0; WORDS], |member| member.inside);
        let desired = membership(&regions, admitted.then_some(player.as_ref()));
        for kind in [EventKind::RegionLeft, EventKind::RegionEntered] {
            for (index, region) in regions.iter().enumerate() {
                let entered = contains(&desired, index);
                if contains(&inside, index) == entered
                    || entered != (kind == EventKind::RegionEntered)
                {
                    continue;
                }
                if available == 0 {
                    break;
                }
                let Some(reg) = catalog
                    .player_lifecycles()
                    .find(|r| r.key == region.service)
                    .cloned()
                else {
                    continue;
                };
                let Some(transition) = state.region_runtime.transition.checked_add(1) else {
                    break;
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
                    reg,
                    Event {
                        kind,
                        profile,
                        transition,
                        player: Some(player.as_ref().clone()),
                        region: Some(region.key.clone()),
                    },
                    final_session,
                );
                inside[index / 64] ^= 1 << (index % 64);
                available -= 1;
            }
        }
        if !admitted && inside.iter().all(|word| *word == 0) {
            state.region_runtime.members.remove(&(profile, session));
        } else {
            state
                .region_runtime
                .members
                .insert((profile, session), Member { player, inside });
        }
    }
}
#[cfg(test)]
mod tests;
