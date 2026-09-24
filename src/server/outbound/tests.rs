use super::*;

fn pong(nonce: u64) -> ServerMessage {
    ServerMessage::Pong { nonce }
}

#[test]
fn frame_admission_includes_the_frame_currently_being_written() {
    let telemetry = Arc::new(OutboundTelemetry::default());
    let (queue, receiver) = telemetry.client_queue_with_limits(2, 64);
    queue.try_send(pong(1)).unwrap();
    queue.try_send(pong(2)).unwrap();
    assert_eq!(queue.try_send(pong(3)), Err(OutboundError::FrameLimit));
    assert_eq!(queue.snapshot().queued_frames, 2);

    let frame = receiver.try_recv().unwrap();
    assert_eq!(queue.snapshot().queued_frames, 2);
    drop(frame);
    assert_eq!(queue.snapshot().queued_frames, 1);
    queue.try_send(pong(4)).unwrap();
    assert_eq!(queue.snapshot().queued_frames, 2);

    drop(receiver);
    assert_eq!(queue.snapshot().queued_frames, 0);
    assert_eq!(queue.snapshot().queued_bytes, 0);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.queued_bytes, 0);
    assert_eq!(snapshot.queued_messages, 0);
    assert_eq!(snapshot.rejections, 1);
}

#[test]
fn per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop() {
    let telemetry = Arc::new(OutboundTelemetry::default());
    let (queue, receiver) = telemetry.client_queue_with_limits(8, 14);
    let clone = queue.clone();
    queue.try_send(pong(1)).unwrap();
    assert_eq!(clone.try_send(pong(2)), Err(OutboundError::ClientByteLimit));
    assert_eq!(queue.snapshot().queued_bytes, 14);

    drop(receiver);
    assert_eq!(queue.snapshot().queued_bytes, 0);
    clone.try_send(pong(3)).unwrap_err();
    assert_eq!(queue.snapshot().queued_bytes, 0);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.queued_bytes, 0);
    assert_eq!(snapshot.queued_messages, 0);
    assert_eq!(snapshot.rejections, 2);
}

#[test]
fn aggregate_byte_limit_is_enforced_across_clients() {
    let telemetry = Arc::new(OutboundTelemetry::with_aggregate_byte_limit(20));
    let (first, first_receiver) = telemetry.client_queue_with_limits(4, 20);
    let (second, second_receiver) = telemetry.client_queue_with_limits(4, 20);
    first.try_send(pong(1)).unwrap();
    assert_eq!(
        second.try_send(pong(2)),
        Err(OutboundError::AggregateByteLimit)
    );
    assert_eq!(telemetry.snapshot().queued_bytes, 14);

    drop(first_receiver);
    second.try_send(pong(3)).unwrap();
    assert_eq!(telemetry.snapshot().queued_bytes, 14);
    drop(second_receiver);
    assert_eq!(telemetry.snapshot().queued_bytes, 0);
}

#[test]
fn aggregate_high_water_mark_survives_sub_tick_queue_drain() {
    let telemetry = Arc::new(OutboundTelemetry::default());
    let (first, first_receiver) = telemetry.client_queue();
    let (second, second_receiver) = telemetry.client_queue();
    first.try_send(pong(1)).unwrap();
    second.try_send(pong(2)).unwrap();
    let peak = telemetry.snapshot();
    assert_eq!(peak.queued_bytes, 28);
    assert_eq!(peak.max_queued_bytes, 28);
    assert_eq!(peak.max_client_queued_bytes, 14);

    drop(first_receiver);
    drop(second_receiver);
    let drained = telemetry.snapshot();
    assert_eq!(drained.queued_bytes, 0);
    assert_eq!(drained.max_queued_bytes, 28);
}

#[test]
fn oversized_and_disconnected_admissions_are_explicit_and_release_reservations() {
    let telemetry = Arc::new(OutboundTelemetry::default());
    let (small, small_receiver) = telemetry.client_queue_with_limits(4, 13);
    assert_eq!(small.try_send(pong(1)), Err(OutboundError::TooLarge));
    assert_eq!(small.snapshot().queued_frames, 0);
    drop(small_receiver);

    let (frame_limited, frame_receiver) = telemetry.client_queue();
    assert_eq!(
        frame_limited.try_send(ServerMessage::ContentManifestPart {
            fingerprint: 0,
            total_len: 70_000,
            offset: 0,
            bytes: vec![0; 70_000],
        }),
        Err(OutboundError::TooLarge)
    );
    assert_eq!(frame_limited.snapshot().queued_frames, 0);
    drop(frame_receiver);

    let (closed, closed_receiver) = telemetry.client_queue_with_limits(4, 64);
    drop(closed_receiver);
    assert_eq!(closed.try_send(pong(2)), Err(OutboundError::Disconnected));
    assert_eq!(closed.snapshot().queued_frames, 0);
    assert_eq!(closed.snapshot().queued_bytes, 0);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.queued_bytes, 0);
    assert_eq!(snapshot.queued_messages, 0);
    assert_eq!(snapshot.rejections, 3);
}
