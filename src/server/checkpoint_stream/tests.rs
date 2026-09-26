use super::*;

#[test]
fn bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix() {
    let mut visited = 0;
    let parts = (0..20).map(|index| {
        visited += 1;
        Ok(vec![index as u8; 10])
    });
    let mut bytes = Vec::new();
    let mut turns = Vec::new();
    let result = write_frame(&mut bytes, 1_000, parts, 3, |count| {
        turns.push(count);
        if turns.len() == 2 {
            Err(io::Error::other("cancelled"))
        } else {
            Ok(())
        }
    });
    assert!(result.is_err());
    assert_eq!(turns, [3, 3]);
    assert_eq!(visited, 6);
    assert_eq!(bytes.len(), 60); // no checksum on partial output

    bytes.clear();
    turns.clear();
    write_frame(
        &mut bytes,
        204,
        (0..20).map(|i| Ok(vec![i; 10])),
        3,
        |count| {
            turns.push(count);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(turns, [3, 3, 3, 3, 3, 3, 2]);
    assert_eq!(&bytes[200..], &0x6527_a64fu32.to_le_bytes());
}

#[test]
fn byte_budget_stops_turns_and_file_limit_does_not_write_overshoot() {
    let mut bytes = Vec::new();
    let mut turns = Vec::new();
    write_frame(
        &mut bytes,
        3 * WRITE_BYTES + 4,
        (0..3).map(|_| Ok(vec![7; WRITE_BYTES])),
        16,
        |count| {
            turns.push(count);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(turns, [1, 1, 1]);
    bytes.clear();
    assert!(write_frame(&mut bytes, 8, [Ok(vec![1; 5])].into_iter(), 1, |_| Ok(())).is_err());
    assert!(bytes.is_empty());
}
