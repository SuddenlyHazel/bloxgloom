use super::*;
use crate::items::ItemId;

#[test]
fn hopper_codec_preserves_components_and_rejects_hidden_slots() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = codec::Codec::<3> {
        catalog: catalog.clone(),
    };
    let item = ItemId(crate::world::STONE.0);
    let stack = Stack::with_components(item, 128, 1, vec![1, 2, 3]).unwrap();
    let payload = HopperPayload {
        slots: [None, Some(stack.clone()), None],
    };
    let bytes = codec.encode(&EntityPayload::new(payload.clone())).unwrap();
    let decoded = codec.decode(&bytes).unwrap();
    assert_eq!(decoded.downcast_ref::<HopperPayload>(), Some(&payload));
    let public = crate::protocol::workstation::WorkstationView::decode(
        &codec.public_view(&decoded).unwrap(),
    )
    .unwrap();
    assert_eq!(public.slots.len(), 3);
    assert_eq!(public.slots[1], Some(Stack::new(item, 128)));
    let mut invalid = Inventory::default();
    invalid.slots[35] = Some(stack);
    assert!(
        codec
            .decode(
                &crate::inventory::InventoryStore::encode_snapshot_with_catalog(&invalid, &catalog)
                    .unwrap()
            )
            .is_err()
    );
    assert!(codec.decode(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn inventory_port_keeps_exact_components_and_refuses_overflow() {
    let catalog = Catalog::builtins();
    let item = ItemId(crate::world::STONE.0);
    let tagged = Stack::with_components(item, 128, 1, vec![42]).unwrap();
    let payload = EntityPayload::new(HopperPayload {
        slots: [
            Some(tagged.clone()),
            Some(Stack::new(item, 128)),
            Some(Stack::new(item, 128)),
        ],
    });
    assert!(
        policy::Port::<HopperPayload>::new()
            .deposit(&payload, &Stack::new(item, 1), &catalog)
            .unwrap()
            .is_none()
    );
    let (after, taken) = policy::Port::<HopperPayload>::new()
        .withdraw(&payload, item, 1, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(taken.components, tagged.components);
    assert_eq!(
        after.downcast_ref::<HopperPayload>().unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        127
    );
    assert!(
        policy::Port::<HopperPayload>::new()
            .deposit(&after, &Stack::new(item, 1), &catalog)
            .unwrap()
            .is_none()
    );
    let restored = policy::Port::<HopperPayload>::new()
        .deposit(&after, &taken, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.downcast_ref::<HopperPayload>(),
        payload.downcast_ref::<HopperPayload>()
    );
}
