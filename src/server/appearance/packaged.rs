//! Profile-owned packaged rig and look selection; playback remains transient.
use super::{State, replace};
use crate::appearance::PackagedAppearance;
use bloxgloom_host_api::entity::{ClipPlayback, VisualState};
use std::io;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
pub(in crate::server) fn select_packaged(
    state: &mut State,
    session: u64,
    selection: Option<PackagedAppearance>,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    let previous = appearance.packaged;
    appearance.packaged = selection.map(|mut p| {
        p = p.durable();
        if let Some(old) = previous.filter(|old| old.model == p.model) {
            p.visual.playback = old.visual.playback;
            p.visual.sample_tick = old.visual.sample_tick;
            p.visual.sequence = old.visual.sequence;
        }
        p
    });
    appearance.character = None;
    replace(state, session, appearance)
}
pub(in crate::server) fn select_model(
    state: &mut State,
    session: u64,
    key: Option<&str>,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    appearance.packaged = match key {
        None => None,
        Some(key) => Some(PackagedAppearance {
            model: state
                .world
                .catalog()
                .player_model_id(key)
                .ok_or_else(|| invalid("unregistered player model"))?,
            visual: VisualState::default(),
        }),
    };
    appearance.character = None;
    replace(state, session, appearance)
}
pub(in crate::server) fn select_visual(
    state: &mut State,
    session: u64,
    visual: VisualState,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    let packaged = appearance
        .packaged
        .as_mut()
        .ok_or_else(|| invalid("select a packaged player model before setting its look"))?;
    // Look operations do not forge playback identity or client-owned clocks.
    let mut visual = visual;
    visual.playback = packaged.visual.playback;
    visual.sequence = packaged.visual.sequence;
    visual.sample_tick = packaged.visual.sample_tick;
    packaged.visual = visual;
    replace(state, session, appearance)
}
pub(in crate::server) fn play_animation(
    state: &mut State,
    session: u64,
    clip: &str,
    speed: f32,
    looping: bool,
    crossfade_s: f32,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    let packaged = appearance
        .packaged
        .as_mut()
        .ok_or_else(|| invalid("scripted animation requires a packaged player model"))?;
    let model = state
        .world
        .catalog()
        .player_model(packaged.model)
        .ok_or_else(|| invalid("unregistered player model"))?;
    let clip = model
        .model
        .clips
        .iter()
        .position(|c| c.name == clip)
        .ok_or_else(|| invalid("unknown player clip"))? as u16;
    let tick = state.player_runtime.current_tick();
    let sequence = packaged
        .visual
        .sequence
        .checked_add(1)
        .ok_or_else(|| invalid("player playback sequence exhausted"))?;
    packaged.visual.sample_tick = tick;
    packaged.visual.sequence = sequence;
    packaged.visual.playback = Some(ClipPlayback {
        clip,
        speed,
        looping,
        crossfade_s,
        started_tick: tick,
        sequence,
    });
    replace(state, session, appearance)
}

pub(in crate::server) fn stop_animation(
    state: &mut State,
    session: u64,
    crossfade_s: f32,
) -> io::Result<()> {
    let mut appearance = state
        .player_entities
        .appearance_state_for_session(session)
        .unwrap_or_default();
    let p = appearance
        .packaged
        .as_mut()
        .ok_or_else(|| invalid("scripted animation requires a packaged player model"))?;
    p.visual.playback = None;
    p.visual.transition_s = crossfade_s;
    replace(state, session, appearance)
}
