//! Late receipt publication must not act on a replacement connection.
use super::*;
use bloxgloom_host_api::gameplay::{PlayerOperation, PlayerOperationKind};

#[test]
fn committed_player_operations_never_follow_profile_to_replacement_session() {
    let save = TestSave::new("player-operation-replacement");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let first = join(&mut state, &mut tick, 17);
    state.remove_client(first.id);
    let replacement = join(&mut state, &mut tick, 17);
    assert!(replacement.action_epoch > first.action_epoch);
    let _ = messages(&replacement);
    for kind in [
        PlayerOperationKind::Message("Old notice".into()),
        PlayerOperationKind::Kick("Old removal".into()),
    ] {
        players::committed(
            &mut state,
            players::Published::operations(vec![PlayerOperation {
                profile: 17,
                session: first.action_epoch,
                kind,
            }])
            .unwrap(),
        )
        .unwrap();
        assert!(state.clients.contains_key(&replacement.id));
        assert!(
            !messages(&replacement)
                .iter()
                .any(|m| matches!(m, ServerMessage::PlayerNotice { .. }))
        );
    }
    players::committed(
        &mut state,
        players::Published::operations(vec![PlayerOperation {
            profile: 17,
            session: replacement.action_epoch,
            kind: PlayerOperationKind::Kick("Current removal".into()),
        }])
        .unwrap(),
    )
    .unwrap();
    assert!(!state.clients.contains_key(&replacement.id));
    assert!(
        !state
            .player_runtime
            .is_admitted(17, replacement.action_epoch)
    );
    assert!(messages(&replacement).iter().any(
        |m| matches!(m,ServerMessage::PlayerNotice {kicked:true,text,..} if text=="Current removal")
    ));
}
