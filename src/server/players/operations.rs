//! Transient session effects apply once after receipt, never to a replacement.
use super::State;
use bloxgloom_host_api::gameplay::{PlayerOperation, PlayerOperationKind};
use std::io;

pub(super) fn apply(state: &mut State, operations: Vec<PlayerOperation>) -> io::Result<()> {
    for operation in operations {
        let Some(id) = state
            .clients
            .iter()
            .find(|(_, c)| c.profile == operation.profile && c.action_epoch == operation.session)
            .map(|(id, _)| *id)
        else {
            tracing::debug!(profile=%operation.profile,session=operation.session,"committed player effect skipped departed session");
            continue;
        };
        let (kicked, text) = match operation.kind {
            PlayerOperationKind::HealthChanged {
                health,
                respawn_position,
            } => {
                super::health::publish(state, id, health, respawn_position)?;
                continue;
            }
            PlayerOperationKind::SessionModifier { key, value } => {
                state.player_modifiers.apply(
                    operation.profile,
                    operation.session,
                    &key,
                    value,
                    state.player_runtime.tick,
                )?;
                continue;
            }
            PlayerOperationKind::Model(model) => {
                crate::server::appearance::packaged::select_model(state, id, model.as_deref())?;
                continue;
            }
            PlayerOperationKind::ModelVisual(visual) => {
                crate::server::appearance::packaged::select_visual(state, id, visual)?;
                continue;
            }
            PlayerOperationKind::Animation {
                clip,
                speed,
                looping,
                crossfade_s,
            } => {
                crate::server::appearance::packaged::play_animation(
                    state,
                    id,
                    &clip,
                    speed,
                    looping,
                    crossfade_s,
                )?;
                continue;
            }
            PlayerOperationKind::StopAnimation(crossfade_s) => {
                crate::server::appearance::packaged::stop_animation(state, id, crossfade_s)?;
                continue;
            }
            PlayerOperationKind::Teleport(position) => {
                crate::server::movement::teleport(state, id, position)?;
                continue;
            }
            PlayerOperationKind::Appearance(palettes) => {
                crate::server::appearance::select(state, id, palettes)?;
                continue;
            }
            PlayerOperationKind::Message(text) => (false, text),
            PlayerOperationKind::Kick(reason) => (true, reason),
        };
        let sent = state.clients[&id].enqueue(crate::protocol::ServerMessage::PlayerNotice {
            profile: operation.profile,
            session: operation.session,
            kicked,
            text,
        });
        if kicked || !sent {
            // Removing the authoritative client immediately cancels movement,
            // timers and admission. The reactor drains the already queued notice
            // before observing outbound-channel closure; the reason is best effort.
            state.remove_client(id);
        }
    }
    Ok(())
}
