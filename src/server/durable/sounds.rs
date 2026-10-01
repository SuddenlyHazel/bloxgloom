//! Stock cues derived only from newly applied committed actions.
use super::CommitAction;
use bloxgloom_host_api::sound::{Event, Kind};
pub(super) fn builtin(state: &crate::server::State, action: &mut CommitAction) {
    if let Some(item) = action.pickups.first() {
        action.sounds.push(cue("pickup", item.position));
    }
    if action.action_id.is_none() || action.receipt_value.is_none() {
        return;
    }
    if let Some(delta) = action.deltas.first() {
        let at = [delta.key.x, delta.key.y, delta.key.z];
        let position = std::array::from_fn(|axis| {
            (at[axis] * crate::world::CHUNK_SIZE as i32 + i32::from(delta.local[axis])) as f32 + 0.5
        });
        action.sounds.push(cue(
            if delta.block == crate::world::AIR {
                "break"
            } else {
                "place"
            },
            position,
        ));
    } else if let Some(client) = action.client_id.and_then(|id| state.clients.get(&id)) {
        action.sounds.push(cue("interact", client.position()));
    }
}
fn cue(name: &str, position: [f32; 3]) -> Event {
    Event {
        owner: "bloxgloom".into(),
        voice: name.into(),
        kind: Kind::Play {
            clip: format!("bloxgloom:{name}"),
            position,
            entity: None,
            gain: 1.0,
            pitch: 1.0,
            looping: false,
        },
    }
}
