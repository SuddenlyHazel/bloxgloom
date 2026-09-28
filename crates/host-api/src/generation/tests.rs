use super::*;

#[test]
fn context_handles_negative_and_extreme_chunk_coordinates() {
    let context = Context {
        seed: 27,
        chunk: [-1, i32::MIN, i32::MAX],
    };
    assert_eq!(
        context.world_position([15, 0, 15]),
        Ok([-1, i64::from(i32::MIN) * 16, i64::from(i32::MAX) * 16 + 15])
    );
    assert_eq!(
        context.world_position([16, 0, 0]),
        Err(GenerationError::OutOfBounds([16, 0, 0]))
    );
    assert_eq!(
        context.world_position([0, -1, 0]),
        Err(GenerationError::OutOfBounds([0, -1, 0]))
    );
    let point = [-17, 9, i64::from(i32::MAX) * 16];
    assert_eq!(context.random_at(point, 5), context.random_at(point, 5));
    assert_ne!(context.random_at(point, 5), context.random_at(point, 6));
}

#[test]
fn bounded_output_preserves_index_order_and_rejects_invalid_writes() {
    let mut output = Output::default();
    assert_eq!(
        output.set([0, 0, 16], "example:stone"),
        Err(GenerationError::OutOfBounds([0, 0, 16]))
    );
    assert_eq!(
        output.finish(),
        Err(GenerationError::OutOfBounds([0, 0, 16]))
    );
    let mut output = Output::default();
    output.set([15, 0, 0], "example:one").unwrap();
    output.set([0, 0, 0], "example:two").unwrap();
    assert_eq!(
        output.writes().collect::<Vec<_>>(),
        vec![(0, "example:two"), (15, "example:one")]
    );
    for _ in 2..MAX_WRITES {
        output.set([0, 0, 0], "example:two").unwrap();
    }
    assert_eq!(
        output.set([0, 0, 0], "example:two"),
        Err(GenerationError::WriteLimit)
    );
    assert_eq!(output.finish(), Err(GenerationError::WriteLimit));
}
