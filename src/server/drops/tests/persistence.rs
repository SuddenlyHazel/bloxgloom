use super::*;
use std::{fs, time::Duration};

#[test]
fn drops_survive_restart_and_are_still_collectible() {
    let root = temp_root("bloxgloom-drops");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([1.0, 2.0, 3.0], item(2), 100, Duration::ZERO);
    drops.spawn([1.0, 2.0, 3.0], item(2), 50, Duration::ZERO);
    assert_eq!(drops.entries.len(), 2);
    drops.save().unwrap();
    let mut loaded = Drops::open(&root).unwrap();
    assert_eq!(
        loaded
            .entries
            .values()
            .map(|entry| entry.item.count)
            .sum::<u16>(),
        150
    );
    assert_spatial_members_match_entries(&loaded);
    let item = loaded.pickup_candidates([1.0, 2.0, 3.0])[0];
    loaded.take(item.id, item.count);
    loaded.save().unwrap();
    assert_eq!(Drops::open(&root).unwrap().entries.len(), 1);
    let mut bytes = fs::read(root.join("drops.bin")).unwrap();
    bytes[HEADER] ^= 1;
    fs::write(root.join("drops.bin"), bytes).unwrap();
    assert!(Drops::open(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ancient_persisted_drop_stays_expired_and_is_not_a_merge_target() {
    let root = temp_root("bloxgloom-drop-ancient");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([1.0, 2.0, 3.0], item(2), 10, Duration::ZERO);
    drops.entries.get_mut(&1).unwrap().created_unix_ms = 1;
    drops.save().unwrap();

    let loaded = Drops::open(&root).unwrap();
    assert!(loaded.pickup_candidates([1.0, 2.0, 3.0]).is_empty());
    assert_eq!(loaded.plan_expired(8).changes[0].id, 1);
    let plan = loaded
        .plan_spawn([1.0, 2.0, 3.0], item(2), 5, Duration::ZERO)
        .unwrap();
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(plan.changes[0].id, 2, "expired entry was merged");
    assert!(plan.changes[0].before.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn component_bearing_drops_keep_payload_across_merge_pickup_and_restart() {
    use crate::inventory::Stack;

    let root = temp_root("bloxgloom-drop-components");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    let first = Stack::with_components(item(2), 3, 7, vec![1, 2, 3]).unwrap();
    let distinct = Stack::with_components(item(2), 2, 7, vec![1, 2, 4]).unwrap();
    for stack in [first.clone(), distinct.clone(), first.clone()] {
        let plan = drops
            .plan_spawn_stack([1.0, 2.0, 3.0], stack, Duration::ZERO)
            .unwrap();
        drops.apply_plan(&plan).unwrap();
    }
    assert_eq!(
        drops.entries.len(),
        2,
        "only identical component payloads merge"
    );
    assert_eq!(drops.stack(1).unwrap().count, 6);
    assert_eq!(drops.stack(1).unwrap().components, first.components);
    let owner = drops.owner_snapshot(1);
    assert_eq!(
        journal::decode_owner(1, &owner).unwrap().4,
        first.components
    );
    drops.save().unwrap();

    let mut loaded = Drops::open(&root).unwrap();
    assert_eq!(loaded.stack(1).unwrap().components, first.components);
    assert_eq!(loaded.stack(2).unwrap().components, distinct.components);
    let take = loaded.plan_take(&[(1, 2)]).unwrap();
    loaded.apply_plan(&take).unwrap();
    assert_eq!(loaded.stack(1).unwrap().count, 4);
    assert_eq!(loaded.stack(1).unwrap().components, first.components);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn world_catalog_validates_and_recovers_wide_mod_item_drop() {
    use crate::content::{Catalog, ItemDef, TextureId};
    use crate::inventory::Stack;
    use std::sync::Arc;

    let root = temp_root("bloxgloom-wide-mod-drop");
    fs::create_dir_all(&root).unwrap();
    let mut catalog = Catalog::builtins();
    let mod_item = item(70_001);
    catalog
        .register_item(ItemDef {
            id: mod_item,
            key: "test:wide_item".into(),
            name: "WIDE ITEM".into(),
            swatch: [0.4, 0.6, 0.8, 1.0],
            texture: TextureId::new(3),
            placeable: None,
            sprite: true,
        })
        .unwrap();
    let catalog = Arc::new(catalog);
    let mut drops = Drops::open_with_catalog(&root, Arc::clone(&catalog)).unwrap();
    let stack = Stack::with_components(mod_item, 5, 2, vec![19, 29]).unwrap();
    let plan = drops
        .plan_spawn_stack([0.0, 4.0, 0.0], stack.clone(), Duration::ZERO)
        .unwrap();
    drops.apply_plan(&plan).unwrap();
    drops.save().unwrap();
    let restored = Drops::open_with_catalog(&root, catalog).unwrap();
    assert_eq!(restored.stack(1), Some(stack));
    assert!(
        Drops::open(&root).is_err(),
        "wrong catalog must fail closed"
    );
    fs::remove_dir_all(root).unwrap();
}
