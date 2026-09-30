//! Transient session effects apply once after receipt, never to a replacement.
use super::State;
use bloxgloom_host_api::gameplay::{PlayerOperation, PlayerOperationKind};

pub(super) fn apply(state: &mut State, operations: Vec<PlayerOperation>) {
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
}
