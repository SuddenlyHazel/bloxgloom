use super::*;

fn chunk(x: i32, y: i32, z: i32) -> ChunkKey {
    world_to_chunk(x, y, z).0
}

fn effects_for(batch: &EffectBatch, owner: ChunkKey) -> &[RoutedEffect] {
    batch
        .owners()
        .iter()
        .find(|group| group.owner == owner)
        .map_or(&[], |group| group.effects.as_slice())
}

fn envelope(tick: u64, source: u64, sequence: u64, effect: Effect) -> EffectEnvelope {
    EffectEnvelope {
        key: OrderKey::new(TickId::new(tick), source, sequence),
        effect,
    }
}

#[test]
fn cell_effects_route_through_chunk_boundaries_and_euclidean_negative_coordinates() {
    let effects = [
        envelope(
            11,
            1,
            0,
            Effect::BlockChanged {
                cell: CellCoord::new(16, 8, 8),
            },
        ),
        envelope(
            11,
            2,
            0,
            Effect::WakeDrop {
                id: 5,
                owner: CellCoord::new(-1, -16, -17).owner(),
            },
        ),
    ];

    let batch = route_effects(effects, EffectLimits::default()).unwrap();
    assert_eq!(effects_for(&batch, chunk(16, 8, 8)).len(), 1);
    assert_eq!(effects_for(&batch, chunk(0, 8, 8)).len(), 1);
    assert_eq!(effects_for(&batch, chunk(-1, -16, -17)).len(), 1);
    assert_eq!(batch.owners().len(), 3);
}

#[test]
fn local_and_cross_chunk_effects_share_the_interaction_commit_barrier_and_stable_order() {
    let local_late = envelope(
        7,
        9,
        0,
        Effect::BlockChanged {
            cell: CellCoord::new(4, 5, 6),
        },
    );
    let local_early = envelope(
        7,
        3,
        0,
        Effect::BlockChanged {
            cell: CellCoord::new(5, 5, 6),
        },
    );
    let local_same_source_later = envelope(
        7,
        3,
        1,
        Effect::BlockChanged {
            cell: CellCoord::new(6, 5, 6),
        },
    );
    let remote = envelope(
        7,
        1,
        0,
        Effect::WakeDrop {
            id: 92,
            owner: chunk(16, 5, 6),
        },
    );

    let first = route_effects(
        [local_late, remote, local_same_source_later, local_early],
        EffectLimits::default(),
    )
    .unwrap();
    let second = route_effects(
        [local_early, local_late, remote, local_same_source_later],
        EffectLimits::default(),
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.commit_phase(), Phase::InteractionCommit);
    let local = effects_for(&first, chunk(4, 5, 6));
    assert_eq!(local.len(), 3);
    assert!(local[0].key < local[1].key);
    assert!(local[1].key < local[2].key);
    assert_eq!(effects_for(&first, chunk(16, 5, 6)).len(), 1);
}

#[test]
fn block_changes_fan_out_to_boundary_face_edge_and_corner_owners() {
    let corner = CellCoord::new(15, 15, 15);
    let owners = block_change_owners(corner);
    assert_eq!(owners.len(), 8);
    for x in [0, 1] {
        for y in [0, 1] {
            for z in [0, 1] {
                assert!(owners.contains(&ChunkKey { x, y, z }));
            }
        }
    }

    let negative_face = block_change_owners(CellCoord::new(-16, 2, 2));
    assert_eq!(
        negative_face,
        vec![
            ChunkKey { x: -2, y: 0, z: 0 },
            ChunkKey { x: -1, y: 0, z: 0 }
        ]
    );

    let batch = route_effects(
        [envelope(2, 4, 0, Effect::BlockChanged { cell: corner })],
        EffectLimits::default(),
    )
    .unwrap();
    assert_eq!(
        batch
            .owners()
            .iter()
            .map(|owner| owner.effects.len())
            .sum::<usize>(),
        8
    );
}

#[test]
fn output_overflow_is_explicit_and_rejects_the_whole_producer_buffer() {
    let mut output = EffectBuffer::new(TickId::new(3), 17, 1).unwrap();
    output
        .emit(Effect::WakeDrop {
            id: 1,
            owner: chunk(0, 0, 0),
        })
        .unwrap();
    assert_eq!(
        output.emit(Effect::WakeDrop {
            id: 2,
            owner: chunk(0, 0, 0),
        }),
        Err(EffectBufferError::Overflow { limit: 1 })
    );
    assert_eq!(
        output.finish(),
        Err(EffectBufferError::Overflow { limit: 1 })
    );

    let events = [envelope(
        3,
        1,
        0,
        Effect::BlockChanged {
            cell: CellCoord::new(15, 4, 4),
        },
    )];
    assert_eq!(
        route_effects(
            events,
            EffectLimits {
                total: 1,
                per_owner: 1,
            },
        ),
        Err(RouteError::TotalOverflow { limit: 1 })
    );

    let per_owner = [
        envelope(
            3,
            1,
            0,
            Effect::WakeDrop {
                id: 1,
                owner: chunk(0, 0, 0),
            },
        ),
        envelope(
            3,
            1,
            1,
            Effect::WakeDrop {
                id: 2,
                owner: chunk(0, 0, 0),
            },
        ),
    ];
    assert_eq!(
        route_effects(
            per_owner,
            EffectLimits {
                total: 2,
                per_owner: 1,
            },
        ),
        Err(RouteError::OwnerOverflow {
            owner: chunk(0, 0, 0),
            limit: 1,
        })
    );
}

#[test]
fn route_rejects_mixed_ticks_duplicate_keys_and_excessive_limits() {
    let one = envelope(
        1,
        2,
        0,
        Effect::WakeDrop {
            id: 1,
            owner: chunk(0, 0, 0),
        },
    );
    let other_tick = envelope(
        2,
        2,
        1,
        Effect::WakeDrop {
            id: 2,
            owner: chunk(1, 0, 0),
        },
    );
    assert_eq!(
        route_effects([one, other_tick], EffectLimits::default()),
        Err(RouteError::MixedTicks { first: 1, other: 2 })
    );

    let duplicated = envelope(
        1,
        2,
        0,
        Effect::WakeDrop {
            id: 2,
            owner: chunk(0, 0, 0),
        },
    );
    assert_eq!(
        route_effects([one, duplicated], EffectLimits::default()),
        Err(RouteError::DuplicateOrderKey {
            owner: chunk(0, 0, 0)
        })
    );

    assert_eq!(
        route_effects(
            [],
            EffectLimits {
                total: MAX_EFFECTS_PER_BATCH + 1,
                per_owner: 1,
            }
        ),
        Err(RouteError::LimitTooLarge {
            requested: MAX_EFFECTS_PER_BATCH + 1,
            maximum: MAX_EFFECTS_PER_BATCH,
        })
    );
}
