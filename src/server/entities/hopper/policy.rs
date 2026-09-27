//! Hopper-specific flow decisions; storage behavior lives in shared policies.
pub(super) use super::super::storage::policy::Port;
use super::*;
use crate::server::voxel_view::VoxelView;
pub(super) struct Planner;
impl EntityTickPolicy for Planner {
    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        catalog: &Catalog,
        _: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let anchor = snapshot.anchor().ok_or(EntityError::WrongOwnership)?;
        let own = snapshot
            .private_payload
            .downcast_ref::<HopperPayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let at = |dy: i32| {
            anchor
                .y
                .checked_add(dy)
                .map(|y| CellCoord::new(anchor.x, y, anchor.z))
        };
        let touches = |e: &EntityPublicView, cell: Option<CellCoord>| match &e.location {
            EntityLocation::Anchored { footprint, .. } => {
                cell.is_some_and(|c| footprint.contains(&c))
            }
            _ => false,
        };
        let mut transfer = None;
        for destination in neighbours.iter().filter(|e| touches(e, at(-1))) {
            for stack in own.slots.iter().flatten() {
                let mut one = stack.clone();
                one.count = 1;
                if neighbours.accepts(destination, &one, catalog) {
                    transfer = Some(EntityItemTransfer {
                        source: destination.id,
                        push: true,
                        item: one.item,
                        count: 1,
                    });
                    break;
                }
            }
        }
        if transfer.is_none() {
            for source in neighbours.iter().filter(|e| touches(e, at(1))) {
                for mut stack in neighbours.offers(source) {
                    stack.count = 1;
                    if Port::<HopperPayload>::new()
                        .deposit(&snapshot.private_payload, &stack, catalog)?
                        .is_some()
                    {
                        transfer = Some(EntityItemTransfer {
                            source: source.id,
                            push: false,
                            item: stack.item,
                            count: 1,
                        });
                        break;
                    }
                }
            }
        }
        Ok(EntityTickPlan {
            payload: None,
            next_tick: Some(tick.checked_add(20).ok_or(EntityError::RevisionExhausted)?),
            anchor_update: None,
            position: None,
            block_states: vec![EntityBlockStateChange {
                cell: anchor,
                before: HOPPER_STATE,
                after: HOPPER_STATE,
            }],
            wakes: vec![],
            transfer,
        })
    }
}
