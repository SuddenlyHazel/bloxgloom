use super::*;

#[test]
fn fixed_clock_bounds_catch_up_without_skipping_tick_ids() {
    let mut clock = FixedStepClock::new();

    let first = clock.advance(Duration::from_millis(100), 2).unwrap();
    assert_eq!(first.ticks, [TickId::new(1), TickId::new(2)]);
    assert_eq!(first.backlog, Duration::from_millis(60));
    assert_eq!(first.backlog_ticks, 3);

    let second = clock.advance(Duration::ZERO, 3).unwrap();
    assert_eq!(
        second.ticks,
        [TickId::new(3), TickId::new(4), TickId::new(5)]
    );
    assert_eq!(second.backlog, Duration::ZERO);
    assert_eq!(second.backlog_ticks, 0);
    assert_eq!(clock.last_tick(), TickId::new(5));
}

#[test]
fn clock_keeps_sub_tick_remainder_and_runs_without_clients() {
    // The scheduling core takes no client count; world activity can keep
    // advancing on a dedicated coordinator when no clients are connected.
    let mut clock = FixedStepClock::new();

    let first = clock.advance(Duration::from_millis(35), 8).unwrap();
    assert_eq!(first.ticks, [TickId::new(1)]);
    assert_eq!(first.backlog, Duration::from_millis(15));
    assert_eq!(first.backlog_ticks, 0);

    let second = clock.advance(Duration::from_millis(5), 8).unwrap();
    assert_eq!(second.ticks, [TickId::new(2)]);
    assert_eq!(second.backlog, Duration::ZERO);
}

#[test]
fn command_queue_orders_by_tick_source_and_sequence_and_keeps_phases_separate() {
    assert_eq!(
        Phase::ALL,
        [
            Phase::InputAuthorization,
            Phase::DurableActions,
            Phase::Simulation,
            Phase::InteractionCommit,
            Phase::Publish,
        ]
    );

    let mut queue = CommandQueue::new(8);
    queue
        .try_push(OrderKey::new(TickId::new(2), 1, 1), "later tick")
        .unwrap();
    queue
        .try_push(OrderKey::new(TickId::new(1), 9, 2), "source 9")
        .unwrap();
    queue
        .try_push(OrderKey::new(TickId::new(1), 3, 2), "source 3 seq 2")
        .unwrap();
    queue
        .try_push(OrderKey::new(TickId::new(1), 3, 1), "source 3 seq 1")
        .unwrap();

    let tick_one = queue.drain_tick(TickId::new(1)).unwrap();
    assert_eq!(
        tick_one
            .iter()
            .map(|command| command.payload)
            .collect::<Vec<_>>(),
        ["source 3 seq 1", "source 3 seq 2", "source 9"]
    );
    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.drain_tick(TickId::new(2)).unwrap()[0].payload,
        "later tick"
    );
    assert_eq!(queue.len(), 0);
}

#[test]
fn command_queue_reports_overflow_and_preserves_the_rejected_command() {
    let mut queue = CommandQueue::new(1);
    queue
        .try_push(OrderKey::new(TickId::new(1), 1, 1), "accepted")
        .unwrap();

    let error = queue
        .try_push(OrderKey::new(TickId::new(1), 2, 1), "rejected")
        .unwrap_err();
    assert!(matches!(
        error,
        QueueError::Full {
            payload: "rejected",
            capacity: 1,
            ..
        }
    ));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.capacity(), 1);

    let ready = queue.drain_tick(TickId::new(1)).unwrap();
    assert_eq!(ready[0].payload, "accepted");
}

#[test]
fn command_queue_rejects_duplicate_keys_and_late_commands_explicitly() {
    let mut queue = CommandQueue::new(4);
    let key = OrderKey::new(TickId::new(1), 7, 42);
    queue.try_push(key, "first copy").unwrap();
    assert!(matches!(
        queue.try_push(key, "duplicate"),
        Err(QueueError::DuplicateKey {
            payload: "duplicate",
            ..
        })
    ));
    queue.drain_tick(TickId::new(1)).unwrap();
    assert!(matches!(
        queue.try_push(key, "late"),
        Err(QueueError::ClosedTick {
            payload: "late",
            closed_through: TickId(1),
            ..
        })
    ));
}

#[test]
fn command_queue_refuses_to_skip_a_tick_with_pending_commands() {
    let mut queue = CommandQueue::new(4);
    queue
        .try_push(OrderKey::new(TickId::new(1), 1, 1), "tick one")
        .unwrap();

    assert_eq!(
        queue.drain_tick(TickId::new(2)),
        Err(DrainError::UndrainedEarlierTick {
            requested: TickId::new(2),
            pending: TickId::new(1),
        })
    );
    assert_eq!(
        queue.drain_tick(TickId::new(1)).unwrap()[0].payload,
        "tick one"
    );
}
