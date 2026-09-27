use super::*;
use crate::inventory::STACK_LIMIT;

fn transfer_to(id: u64) -> EntityItemTransfer {
    EntityItemTransfer {
        source: EntityId::new(id).unwrap(),
        push: false,
        item: crate::items::ItemId(7),
        count: 30,
    }
}

#[test]
fn transfer_intent_validates_its_declared_shape() {
    let receiver = EntityId::new(2).unwrap();
    transfer_to(1).validate(receiver).unwrap();
    // Deterministic: same inputs, same outcome.
    transfer_to(1).validate(receiver).unwrap();
}

#[test]
fn transfer_intent_rejects_self_pull_and_bad_counts() {
    let receiver = EntityId::new(2).unwrap();
    assert_eq!(
        EntityItemTransfer {
            source: receiver,
            push: false,
            item: crate::items::ItemId(7),
            count: 1,
        }
        .validate(receiver),
        Err(EntityError::InvalidPayload)
    );
    for count in [0, STACK_LIMIT + 1, u16::MAX] {
        assert_eq!(
            EntityItemTransfer {
                source: EntityId::new(1).unwrap(),
                push: false,
                item: crate::items::ItemId(7),
                count,
            }
            .validate(receiver),
            Err(EntityError::InvalidPayload)
        );
    }
}
