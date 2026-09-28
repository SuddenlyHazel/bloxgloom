//! Reaction removals use the production worker dispatch and receipt barrier.
use super::*;
use bloxgloom_host_api::{
    Extension, Registrar, RegistrationError,
    gameplay::{Context, Error, Event, EventKind, Handler, HandlerRegistration, RemovalCause},
};

struct SoilPost {
    decision: bool,
}
struct SoilRegistration<'a>(&'a mut dyn Registrar);
impl Registrar for SoilRegistration<'_> {
    fn cube_block(&mut self, _: bloxgloom_host_api::CubeBlock) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn inventory_screen(
        &mut self,
        _: bloxgloom_host_api::InventoryScreen,
    ) -> Result<(), RegistrationError> {
        unreachable!()
    }
    fn storage_block_entity(
        &mut self,
        _: bloxgloom_host_api::StorageBlockEntity,
    ) -> Result<(), RegistrationError> {
        unreachable!()
    }
    fn anchored_block_entity(
        &mut self,
        mut definition: bloxgloom_host_api::anchored::AnchoredBlockEntity,
    ) -> Result<(), RegistrationError> {
        definition.block = "bloxgloom:grass".into();
        definition.placement_item = "bloxgloom:grass".into();
        definition.anchor_state = "bloxgloom:grass".into();
        for cell in &mut definition.footprint {
            cell.state = "bloxgloom:grass".into();
        }
        self.0.anchored_block_entity(definition)
    }
}
struct RemovalDecision;
impl Handler for RemovalDecision {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved { cell, cause, .. } = event else {
            return Err(Error::Invalid("expected removal".into()));
        };
        assert_eq!(*cause, RemovalCause::AnchoredBreak);
        assert!(context.player().is_none());
        assert_eq!(context.block(*cell)?.block_type, "bloxgloom:air");
        assert_eq!(
            context.block([cell[0], cell[1] + 1, cell[2]])?.block_type,
            "bloxgloom:air",
            "the complete footprint must be staged before decisions"
        );
        // Read-only seam dependency, separate from the written footprint chunk.
        let remote = context.block([cell[0] + 8, cell[1], cell[2]])?;
        if remote.block_type == "bloxgloom:stone" {
            context.set_block(*cell, "bloxgloom:grass")?;
        }
        context.spawn_drop(cell.map(|v| v as f32 + 0.5), "bloxgloom:seeds", 1, 250)
    }
}
impl Extension for SoilPost {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        bloxgloom_lifecycle_fixture::anchored::SignalPost
            .register(&mut SoilRegistration(registrar))?;
        if self.decision {
            registrar.gameplay_handler(HandlerRegistration {
                key: "test:reaction_removal".into(),
                version: 1,
                event: EventKind::BlockRemoved,
                target: Some("bloxgloom:grass".into()),
                handler: Arc::new(RemovalDecision),
            })?;
        }
        Ok(())
    }
}
fn open_soil(path: &std::path::Path, decision: bool) -> State {
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&SoilPost { decision })
        .unwrap();
    crate::server::server_state_with_startup(53, path.to_path_buf(), 8, startup).unwrap()
}
fn drops(state: &State, item: ItemId) -> u16 {
    crate::server::drops::nearby(&state.entities, [-0.5, 80.5, -0.5])
        .iter()
        .filter_map(|drop| {
            crate::server::drops::stack(&state.entities, EntityId::new(drop.id).unwrap())
        })
        .filter(|stack| stack.item == item)
        .map(|stack| stack.count)
        .sum()
}
fn stage_reaction(
    state: &mut State,
    action: &CommitAction,
) -> Result<bool, crate::server::durable::StageError> {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    state
        .durability
        .try_stage(TickId::new(31), action, Some(permit))
}

#[test]
fn reaction_removal_decisions_support_loss_and_refund_share_receipt_and_recovery() {
    // The unoverridden case protects native harvest suppression: only the
    // lifecycle refund is due, not a second item for each footprint cell.
    for decision in [false, true] {
        let path = temp_save_dir("reaction-gameplay-removal");
        let mut state = open_soil(&path, decision);
        resident(&mut state);
        state.world.edit(-1, 78, -1, crate::world::STONE).unwrap();
        // Establish fixture terrain before the first journaled edit. Mutating
        // this chunk directly after a receipt would break its WAL preimage.
        state.world.edit(-1, 81, -1, RED_FLOWER).unwrap();
        state.world.edit(7, 79, -1, crate::world::AIR).unwrap();
        let soil = state
            .world
            .catalog()
            .item_by_key("bloxgloom:grass")
            .unwrap();
        let flower = state
            .world
            .catalog()
            .item_by_key("bloxgloom:red_flower")
            .unwrap();
        let seeds = state
            .world
            .catalog()
            .item_by_key("bloxgloom:seeds")
            .unwrap();
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(soil, 3));
        let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
        let epoch = u128::from(grant_action_epoch(&mut state, 17)) << 64;
        command(&mut state, edit(epoch | 1, 79, GRASS), 10);
        let id = state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .unwrap();
        command(&mut state, edit(epoch | 2, 78, crate::world::AIR), 30);
        let action = tick_action(&mut state, id, 31);
        assert!(
            action
                .changed_cells
                .contains(&crate::server::effects::CellCoord::new(-1, 81, -1))
        );
        assert_eq!(state.world.cached_block(-1, 81, -1), Some(RED_FLOWER));
        assert!(state.entities.snapshot(id).is_some());
        assert_eq!(drops(&state, soil), 0);
        assert_eq!(drops(&state, flower), 0);

        if decision {
            // Reject a stale decision read before WAL admission, then replan
            // with the changed read. A handler cannot restore a removed cell.
            state.world.edit(7, 79, -1, crate::world::STONE).unwrap();
            assert!(matches!(
                stage_reaction(&mut state, &action),
                Err(crate::server::durable::StageError::Conflict)
            ));
            let result = crate::server::durable::entity_dispatch::plan_motion(
                &mut state,
                TickId::new(31),
                vec![DurableRequest::EntityTick { id }],
            )
            .pop()
            .unwrap()
            .1;
            assert_eq!(result.err().unwrap().kind(), ErrorKind::PermissionDenied);
            assert!(state.durability.pending.is_empty());
            assert!(state.entities.snapshot(id).is_some());
            assert_eq!(state.world.cached_block(-1, 81, -1), Some(RED_FLOWER));
            assert_eq!(drops(&state, seeds), 0);
            state.world.edit(7, 79, -1, crate::world::AIR).unwrap();
        }
        let action = tick_action(&mut state, id, 31);
        assert!(stage_reaction(&mut state, &action).unwrap());
        assert_eq!(state.durability.pending.len(), 1);
        assert!(state.entities.snapshot(id).is_some());
        assert_eq!(state.world.cached_block(-1, 80, -1), Some(GRASS));
        assert_eq!(state.world.cached_block(-1, 81, -1), Some(RED_FLOWER));
        assert_eq!(drops(&state, soil), 0);
        crate::server::durable::complete_barrier(
            &mut state,
            crate::server::durable::CommitBarrier::AllStaged,
        )
        .unwrap();
        assert!(state.entities.snapshot(id).is_none());
        for y in 79..=81 {
            assert_eq!(state.world.cached_block(-1, y, -1), Some(crate::world::AIR));
        }
        assert_eq!(drops(&state, soil), 2);
        assert_eq!(drops(&state, flower), 1);
        assert_eq!(drops(&state, seeds), u16::from(decision));
        assert_eq!(state.clients[&1].inventory.slots[0], None);
        drop(peer);
        drop(state);

        let mut state = open_soil(&path, decision);
        resident(&mut state);
        assert!(state.entities.snapshot(id).is_none());
        for y in 79..=81 {
            assert_eq!(state.world.cached_block(-1, y, -1), Some(crate::world::AIR));
        }
        assert!(
            plan_durable_request(
                &mut state,
                &DurableRequest::EntityTick { id },
                TickId::new(32),
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(drops(&state, soil), 2);
        assert_eq!(drops(&state, flower), 1);
        assert_eq!(drops(&state, seeds), u16::from(decision));
        assert_eq!(state.inventory_store.load(17).unwrap().slots[0], None);
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }
}
