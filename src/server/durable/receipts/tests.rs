use super::*;

fn record(sequence: u64, accepted: bool) -> ResultRecord {
    ResultRecord {
        spawned: Default::default(),
        payload: vec![2, 1, 0, 1, sequence as u8, 0],
        accepted,
        reason: if accepted {
            String::new()
        } else {
            "rejected".into()
        },
    }
}

fn id(epoch: u64, sequence: u64) -> u128 {
    (u128::from(epoch) << 64) | u128::from(sequence)
}

#[test]
fn ack_retires_both_outcomes_without_reopening_sequence() {
    let mut ledger = ReceiptLedger::default().grant_next_epoch().unwrap();
    for sequence in 1..=2 {
        ledger = ledger
            .append_result(record(sequence, sequence == 1))
            .unwrap();
    }
    assert!(matches!(
        ledger.admission(id(1, 1), &record(1, true).payload),
        Admission::Replay(_)
    ));
    assert!(matches!(
        ledger.admission(id(1, 2), &record(2, false).payload),
        Admission::Replay(ResultRecord {
            accepted: false,
            ..
        })
    ));
    ledger = ledger.acknowledge(1, 2).unwrap().unwrap();
    assert!(ledger.results.is_empty());
    assert!(matches!(
        ledger.admission(id(1, 1), &record(1, true).payload),
        Admission::Retired
    ));
    assert!(matches!(
        ledger.admission(id(1, 2), &record(2, false).payload),
        Admission::Retired
    ));
    assert!(matches!(
        ledger.admission(id(1, 3), &record(3, true).payload),
        Admission::New
    ));
    assert_eq!(
        ReceiptLedger::decode(&ledger.encode().unwrap()).unwrap(),
        ledger
    );
}

#[test]
fn reconnect_closes_old_epoch_and_preserves_monotonic_epoch_after_codec() {
    let ledger = ReceiptLedger::default()
        .grant_next_epoch()
        .unwrap()
        .append_result(record(1, true))
        .unwrap();
    let ledger = ReceiptLedger::decode(&ledger.encode().unwrap()).unwrap();
    let next = ledger.grant_next_epoch().unwrap();
    assert_eq!(next.current_epoch(), 2);
    assert!(matches!(
        next.admission(id(1, 1), &record(1, true).payload),
        Admission::WrongEpoch
    ));
    assert!(matches!(
        next.admission(id(2, 1), &record(1, true).payload),
        Admission::New
    ));
}

#[test]
fn bounded_window_can_run_beyond_old_lifetime_cap() {
    let mut ledger = ReceiptLedger::default().grant_next_epoch().unwrap();
    for sequence in 1..=WINDOW as u64 {
        ledger = ledger.append_result(record(sequence, true)).unwrap();
    }
    assert!(matches!(
        ledger.admission(id(1, WINDOW as u64 + 1), &record(1, true).payload),
        Admission::Full
    ));
    ledger = ledger.acknowledge(1, WINDOW as u64).unwrap().unwrap();
    for sequence in WINDOW as u64 + 1..=1_000_001 {
        ledger = ledger
            .append_result(record(sequence, sequence % 2 == 0))
            .unwrap();
        ledger = ledger.acknowledge(1, sequence).unwrap().unwrap();
    }
    assert!(ledger.results.is_empty());
    assert_eq!(ledger.acknowledged, 1_000_001);
    assert!(matches!(
        ledger.admission(id(1, 1), &record(1, true).payload),
        Admission::Retired
    ));
    assert_eq!(
        ReceiptLedger::decode(&ledger.encode().unwrap()).unwrap(),
        ledger
    );
}

#[test]
fn malformed_ack_and_tampered_snapshot_fail_closed() {
    let ledger = ReceiptLedger::default().grant_next_epoch().unwrap();
    assert!(ledger.acknowledge(1, 1).is_err());
    assert!(ledger.acknowledge(2, 0).is_err());
    let mut bytes = ledger.encode().unwrap();
    bytes[22] ^= 1;
    assert!(ReceiptLedger::decode(&bytes).is_err());
}

#[test]
fn committed_launch_ids_survive_restart_and_duplicate_action_admission() {
    let mut result = record(1, true);
    result.spawned = vec![
        crate::protocol::SpawnReceipt {
            ordinal: 0,
            entity: 5,
        },
        crate::protocol::SpawnReceipt {
            ordinal: 1,
            entity: 9,
        },
    ];
    let ledger = ReceiptLedger::default()
        .grant_next_epoch()
        .unwrap()
        .append_result(result.clone())
        .unwrap();
    let recovered = ReceiptLedger::decode(&ledger.encode().unwrap()).unwrap();
    let Admission::Replay(actual) = recovered.admission(id(1, 1), &result.payload) else {
        panic!("expected durable replay")
    };
    assert_eq!(actual.spawned, result.spawned);
    let mut rejected = result;
    rejected.accepted = false;
    rejected.reason = "rejected".into();
    assert!(
        ReceiptLedger::default()
            .grant_next_epoch()
            .unwrap()
            .append_result(rejected)
            .is_err()
    );
}
