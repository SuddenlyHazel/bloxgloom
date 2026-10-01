//! Actual packaged startup/callback and typed serialization, without networking.
use super::*;
use crate::client::presentation::{ActionView, InventoryView, Observations, WorldView};
use crate::config::bindings::{Bindings, NamedBindings};
use crate::inventory::{Inventory, Stack};
use winit::keyboard::KeyCode;

fn settle(session: &mut Session) {
    for _ in 0..16 {
        session.poll_presentation();
        session.wait_for_presentation().unwrap();
    }
}
fn text<'a>(session: &'a Session, id: &str) -> &'a str {
    session.text_at(session.node_index(id).unwrap())
}

#[test]
fn typed_recipe_browser_uses_exact_inventory_components_clock_and_own_receipts() {
    let snapshot = crate::server::PackageSnapshot::discover(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe-browser/packages"),
    )
    .unwrap();
    let bundle = Arc::clone(snapshot.client_bundle());
    let resources = Arc::clone(bundle.ui().unwrap());
    let startup = crate::client::startup::prepare(Arc::clone(&bundle)).unwrap();
    let mut session = Session::with_startup(Arc::clone(&resources), startup.clone());
    session.resize(1280, 720, 1.0);
    assert_eq!(
        session.binding_key(
            KeyCode::KeyB,
            &NamedBindings::default(),
            Bindings::default(),
            false,
            false
        ),
        Some(true)
    );
    settle(&mut session);
    assert_eq!(
        text(&session, "recipe:browser/availability"),
        "Inventory: waiting for server"
    );
    assert_eq!(
        text(&session, "recipe:browser/world"),
        "World: waiting for server"
    );

    let catalog = crate::content::Catalog::builtins();
    let stone = catalog.item_by_key("bloxgloom:stone").unwrap();
    let gravel = catalog.item_by_key("bloxgloom:gravel").unwrap();
    let mut inventory = Inventory {
        revision: (1u64 << 53) + 5,
        ..Default::default()
    };
    inventory.slots[0] = Some(Stack::new(stone, 6));
    inventory.slots[1] = Some(Stack::new(gravel, 127));
    inventory.slots[2] = Stack::with_components(stone, 4, 1, vec![0, 255]);
    for slot in &mut inventory.slots[3..] {
        *slot = Some(Stack::new(crate::items::STICK, 128));
    }
    let before = inventory.clone();
    let mut observations = Observations {
        inventory: Some(InventoryView::from_inventory(&inventory, &catalog).unwrap()),
        world: Some(WorldView {
            elapsed_ms: 300012,
            cycle_ms: crate::daylight::CYCLE_MS,
        }),
        ..Default::default()
    };
    session.observe(
        "replica:inventory",
        "deliberately unstructured summary".into(),
        Arc::new(observations.clone()),
    );
    settle(&mut session);
    assert_eq!(
        text(&session, "recipe:browser/availability"),
        "Plain input: 6; max from a matching slot: 6"
    );
    assert_eq!(
        text(&session, "recipe:browser/components"),
        "Tagged input excluded: 4; v1 bytes 0/255"
    );
    assert_eq!(
        text(&session, "recipe:browser/world"),
        "World cycle: 300012/1200000 ms"
    );
    assert!(
        session.take_action_request().is_none(),
        "replica callbacks cannot request crafting"
    );

    let quantity = session.node_index("recipe:browser/quantity").unwrap();
    session.apply_egui(EguiIntent::Input(quantity, "2".into()));
    settle(&mut session);
    let craft = session.node_index("recipe:browser/craft").unwrap();
    session.apply_egui(EguiIntent::Activate(craft));
    settle(&mut session);
    assert_eq!(
        session.take_action_request(),
        Some(("recipe:craft".into(), b"stone:2".to_vec()))
    );
    assert_eq!(
        inventory, before,
        "presentation never debits the authoritative inventory"
    );

    let own_id = (3u128 << 64) | 1;
    observations.actions.push(ActionView {
        spawned: Default::default(),
        id: own_id,
        key: Some("recipe:craft".into()),
        accepted: false,
        reason: "No room".into(),
    });
    observations.actions.push(ActionView {
        spawned: Default::default(),
        id: (3u128 << 64) | 2,
        key: Some("other:private".into()),
        accepted: true,
        reason: String::new(),
    });
    session.observe(
        "replica:action",
        "accepted=true".into(),
        Arc::new(observations.clone()),
    );
    settle(&mut session);
    assert_eq!(
        text(&session, "recipe:browser/receipt"),
        "Craft receipt: denied"
    );
    assert_eq!(
        text(&session, "recipe:browser/receipt_id"),
        format!("{own_id:032x}")
    );
    assert!(session.take_action_request().is_none());

    inventory.slots[0].as_mut().unwrap().count = 5;
    inventory.slots[1].as_mut().unwrap().count = 128;
    inventory.revision += 1;
    observations.inventory = Some(InventoryView::from_inventory(&inventory, &catalog).unwrap());
    observations.actions[0].accepted = true;
    observations.actions[0].reason.clear();
    session.observe(
        "replica:inventory",
        "items=incorrect summary".into(),
        Arc::new(observations.clone()),
    );
    settle(&mut session);
    assert_eq!(
        text(&session, "recipe:browser/availability"),
        "Plain input: 5; max from a matching slot: 5"
    );
    assert_eq!(
        text(&session, "recipe:browser/receipt"),
        "Craft receipt: accepted"
    );

    assert_eq!(
        text(&session, "recipe:browser/receipt_id"),
        format!("{own_id:032x}")
    );

    // Consuming the whole input creates output space even when every other
    // slot is full. The browser must account for the post-consumption inventory.
    let mut completed = inventory.clone();
    for _ in 0..5 {
        assert!(completed.consume(0, stone));
    }
    assert_eq!(completed.insert_with_catalog(gravel, 5, &catalog), 0);
    assert_eq!(completed.slots[0].as_ref().unwrap().count, 5);
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 5);

    // A stack larger than the recipe's eight-item limit cannot be fully
    // consumed, so it cannot create that otherwise unavailable output slot.
    inventory.slots[0].as_mut().unwrap().count = 9;
    inventory.revision += 1;
    observations.inventory = Some(InventoryView::from_inventory(&inventory, &catalog).unwrap());
    session.observe("replica:inventory", "".into(), Arc::new(observations));
    settle(&mut session);
    assert_eq!(
        text(&session, "recipe:browser/availability"),
        "Plain input: 9; max from a matching slot: 0"
    );

    let mut replacement = Session::with_startup(resources, startup);
    assert_eq!(
        replacement.binding_key(
            KeyCode::KeyB,
            &NamedBindings::default(),
            Bindings::default(),
            false,
            false
        ),
        Some(true)
    );
    settle(&mut replacement);
    assert_eq!(
        text(&replacement, "recipe:browser/availability"),
        "Inventory: waiting for server"
    );
    assert_eq!(
        text(&replacement, "recipe:browser/receipt"),
        "No craft receipt yet"
    );
}
