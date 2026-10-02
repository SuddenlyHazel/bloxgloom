use super::*;
use crate::protocol::{
    ServerMessage, read_server_with_catalog, server_wire_len, write_server_with_catalog,
};

fn drop(stack: Stack) -> DroppedItem {
    DroppedItem {
        id: 7,
        item: stack.item,
        count: stack.count,
        components: stack.components,
        position: [1., 2., 3.],
        age_ms: 6000,
    }
}

#[test]
fn component_drops_and_pickups_round_trip_exact_catalog_validated_stack_and_lengths() {
    let (_, catalog, plain) = crate::client::item_visuals::tests::example();
    let item = drop(Stack::with_components(plain.item, 128, 1, vec![0, 255, 9]).unwrap());
    for message in [
        ServerMessage::Drops {
            revision: 17,
            items: vec![item.clone()],
        },
        ServerMessage::Pickups {
            items: vec![item.clone()],
        },
    ] {
        let mut bytes = Vec::new();
        write_server_with_catalog(&mut bytes, &message, &catalog).unwrap();
        assert_eq!(bytes.len(), server_wire_len(&message));
        let decoded = read_server_with_catalog(bytes.as_slice(), &catalog).unwrap();
        match decoded {
            ServerMessage::Drops { revision, items } => {
                assert_eq!(revision, 17);
                assert_eq!(items.as_slice(), std::slice::from_ref(&item));
            }
            ServerMessage::Pickups { items } => {
                assert_eq!(items.as_slice(), std::slice::from_ref(&item))
            }
            _ => panic!("wrong message"),
        }
        let component_header = bytes.len() - 3 - 4;
        for (offset, replacement) in [
            (component_header, 2u16),
            (component_header + 2, 9u16),
            (component_header + 2, 1025u16),
        ] {
            let mut bad = bytes.clone();
            bad[offset..offset + 2].copy_from_slice(&replacement.to_le_bytes());
            assert!(read_server_with_catalog(bad.as_slice(), &catalog).is_err());
        }
        let mut truncated = bytes.clone();
        truncated.pop();
        assert!(read_server_with_catalog(truncated.as_slice(), &catalog).is_err());
    }
    for stack in [
        Stack::with_components(plain.item, 1, 2, vec![255]).unwrap(),
        Stack::with_components(plain.item, 1, 1, vec![255; 9]).unwrap(),
        Stack::with_components(crate::items::STICK, 129, 1, vec![255]).unwrap(),
    ] {
        assert!(
            write_server_with_catalog(
                Vec::new(),
                &ServerMessage::Pickups {
                    items: vec![drop(stack)]
                },
                &catalog
            )
            .is_err()
        );
    }
    let mut bytes = Vec::new();
    write_server_with_catalog(
        &mut bytes,
        &ServerMessage::Pickups {
            items: vec![drop(plain)],
        },
        &catalog,
    )
    .unwrap();
    let header = bytes.len() - 4;
    bytes[header..header + 2].copy_from_slice(&1u16.to_le_bytes());
    assert!(
        read_server_with_catalog(bytes.as_slice(), &catalog).is_err(),
        "empty payload must have version zero"
    );
}

#[test]
fn component_drop_snapshot_and_pickup_pages_fit_frames_without_truncating_payloads() {
    let catalog = Catalog::builtins();
    let items: Vec<_> = (1..=256)
        .map(|id| DroppedItem {
            id,
            ..drop(
                Stack::with_components(crate::items::STICK, 128, 1, vec![255; MAX_COMPONENT_BYTES])
                    .unwrap(),
            )
        })
        .collect();
    let count = snapshot_count(&items);
    assert_eq!(count, 61);
    let snapshot = ServerMessage::Drops {
        revision: 1,
        items: items[..count].to_vec(),
    };
    write_server_with_catalog(Vec::new(), &snapshot, &catalog).unwrap();
    assert!(server_wire_len(&snapshot) <= MAX_FRAME + 4);
    assert!(
        write_server_with_catalog(
            Vec::new(),
            &ServerMessage::Drops {
                revision: 1,
                items: items[..count + 1].to_vec()
            },
            &catalog
        )
        .is_err()
    );
    let mut received = Vec::new();
    for page in pickup_pages(&items) {
        let message = ServerMessage::Pickups {
            items: page.to_vec(),
        };
        let mut bytes = Vec::new();
        write_server_with_catalog(&mut bytes, &message, &catalog).unwrap();
        assert_eq!(bytes.len(), server_wire_len(&message));
        assert!(bytes.len() <= MAX_FRAME + 4);
        if let ServerMessage::Pickups { items } =
            read_server_with_catalog(bytes.as_slice(), &catalog).unwrap()
        {
            received.extend(items);
        }
    }
    assert_eq!(
        received, items,
        "paging must conserve every exact pickup stack"
    );
}
