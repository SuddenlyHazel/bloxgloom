use super::*;

#[test]
fn chunk_stream_defers_before_filling_reliable_outbound_queue() {
    let messages = [
        ServerMessage::Pong { nonce: 1 },
        ServerMessage::Pong { nonce: 2 },
    ];
    let bytes = messages
        .iter()
        .map(crate::protocol::server_wire_len)
        .sum::<usize>() as u64;
    assert!(can_stream_snapshot(OutboundClientSnapshot::default(), &messages).unwrap());
    assert!(
        !can_stream_snapshot(
            OutboundClientSnapshot {
                queued_frames: OUTBOUND_FRAME_CAPACITY - SNAPSHOT_FRAME_HEADROOM - 1,
                queued_bytes: 0,
            },
            &messages,
        )
        .unwrap()
    );
    assert!(
        !can_stream_snapshot(
            OutboundClientSnapshot {
                queued_frames: 0,
                queued_bytes: OUTBOUND_CLIENT_BYTE_CAPACITY - SNAPSHOT_BYTE_HEADROOM - bytes + 1,
            },
            &messages,
        )
        .unwrap()
    );
}
