use super::*;
use crate::inventory::STACK_LIMIT;

fn transfer_to(id: u64) -> EntityItemTransfer {
    EntityItemTransfer {
        route: None,
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
            route: None,
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
                route: None,
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

#[test]
fn live_slot_move_uses_current_item_and_clamps_to_capacity_without_loss() {
    let stone = ItemId(crate::world::STONE.0);
    let dirt = ItemId(crate::world::DIRT.0);
    let mut source = Some(Stack::new(stone, 3));
    let mut destination = Some(Stack::new(stone, 127));
    assert_eq!(move_up_to(&mut source, &mut destination, 128), Some(1));
    assert_eq!(source.as_ref().unwrap().count, 2);
    assert_eq!(destination.as_ref().unwrap().count, 128);
    assert_eq!(move_up_to(&mut source, &mut destination, 128), None);

    let mut destination = None;
    source = Some(Stack::new(dirt, 2));
    assert_eq!(move_up_to(&mut source, &mut destination, 128), Some(2));
    assert!(source.is_none());
    assert_eq!(destination, Some(Stack::new(dirt, 2)));

    let mut source = Some(Stack::new(stone, 2));
    assert_eq!(move_up_to(&mut source, &mut destination, 128), None);
    assert_eq!(source, Some(Stack::new(stone, 2)));
    assert_eq!(destination, Some(Stack::new(dirt, 2)));

    let mut source = Some(Stack::with_components(stone, 4, 1, vec![7]).unwrap());
    let mut destination = Some(Stack::with_components(stone, 126, 1, vec![8]).unwrap());
    assert_eq!(move_up_to(&mut source, &mut destination, 128), None);
    assert_eq!(source.as_ref().unwrap().count, 4);
    destination = Some(Stack::with_components(stone, 126, 1, vec![7]).unwrap());
    assert_eq!(move_up_to(&mut source, &mut destination, 128), Some(2));
    assert_eq!(source.as_ref().unwrap().count, 2);
    assert_eq!(destination.as_ref().unwrap().count, 128);
}
