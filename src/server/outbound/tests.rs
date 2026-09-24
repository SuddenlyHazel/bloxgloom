use super::*;
use std::sync::mpsc;

#[test]
fn queued_frames_release_metrics_on_receive_reject_and_receiver_drop() {
    let telemetry = Arc::new(OutboundTelemetry::default());
    let (sender, receiver) = mpsc::sync_channel(1);
    let message = ServerMessage::Pong { nonce: 4 };
    let bytes = protocol::server_wire_len(&message) as u64;
    assert!(telemetry.try_send(&sender, message));
    assert!(!telemetry.try_send(&sender, ServerMessage::Pong { nonce: 5 }));
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.queued_bytes, bytes);
    assert_eq!(snapshot.queued_messages, 1);
    assert_eq!(snapshot.rejections, 1);

    let frame = receiver.try_recv().unwrap();
    assert!(matches!(
        frame.into_message(),
        ServerMessage::Pong { nonce: 4 }
    ));
    assert_eq!(telemetry.snapshot().queued_messages, 0);
    assert!(telemetry.try_send(&sender, ServerMessage::Pong { nonce: 6 }));
    drop(receiver);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.queued_bytes, 0);
    assert_eq!(snapshot.queued_messages, 0);
    assert_eq!(snapshot.sent_bytes, 0);
}
