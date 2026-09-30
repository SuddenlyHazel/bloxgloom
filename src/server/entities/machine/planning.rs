use super::super::transfer::{put, take};
use super::*;
use crate::server::voxel_view::VoxelView;
impl Adapter {
    pub(super) fn process(&self, p: &mut MachinePayload) -> Result<(), EntityError> {
        let d = self
            .definition
            .process
            .as_ref()
            .ok_or(EntityError::InvalidPayload)?;
        let input = p.slots[d.input as usize].as_ref();
        let recipe = input.and_then(|s| self.recipe(s).filter(|r| s.count >= r.input_count));
        let Some(recipe) = recipe else {
            p.progress = 0;
            p.progress_item = None;
            p.fuel = p.fuel.saturating_sub(1);
            return Ok(());
        };
        let item = self.item(&recipe.input)?;
        if p.progress_item != Some(item) {
            p.progress_item = Some(item);
            p.progress = 0;
        }
        let mut output = Stack::new(self.item(&recipe.output)?, recipe.output_count);
        output.components = match &recipe.output_components {
            api::ComponentOutput::Empty => None,
            api::ComponentOutput::PreserveInput => input.unwrap().components.clone(),
            api::ComponentOutput::Exact(v) => Some(Arc::new(
                crate::inventory::ComponentPayload::new(v.version, v.bytes.clone())
                    .ok_or(EntityError::InvalidPayload)?,
            )),
        };
        let mut destination = p.slots[d.output as usize].clone();
        if !self.accepts_slot(usize::from(d.output), &output) || !put(&mut destination, &output) {
            p.fuel = p.fuel.saturating_sub(1);
            return Ok(());
        }
        if let Some(fuel_slot) = d.fuel {
            if p.fuel == 0 {
                let Some(fuel) = p.slots[fuel_slot as usize].as_ref() else {
                    return Ok(());
                };
                let Some(value) = self.fuel(fuel) else {
                    return Ok(());
                };
                p.fuel = value;
                take(&mut p.slots[fuel_slot as usize], 1).ok_or(EntityError::InvalidPayload)?;
            }
            p.fuel = p.fuel.saturating_sub(1);
        }
        p.progress += 1;
        if p.progress >= recipe.pulses {
            take(&mut p.slots[d.input as usize], recipe.input_count)
                .ok_or(EntityError::InvalidPayload)?;
            p.slots[d.output as usize] = destination;
            p.progress = 0;
            if p.slots[d.input as usize].is_none() {
                p.progress_item = None;
            }
        }
        Ok(())
    }
}
impl EntityTickPolicy for Adapter {
    fn read_radius_chunks(&self) -> u8 {
        self.definition.read_radius
    }
    fn reads_neighbours(&self) -> bool {
        self.definition.reads_neighbours
    }
    fn plan(
        &self,
        snapshot: &EntitySnapshot,
        tick: u64,
        _: &Catalog,
        _: &VoxelView,
        neighbours: &EntityView,
    ) -> Result<EntityTickPlan, EntityError> {
        let before = snapshot
            .private_payload
            .downcast_ref::<MachinePayload>()
            .ok_or(EntityError::InvalidPayload)?;
        let due = snapshot.next_tick.ok_or(EntityError::InvalidPayload)?;
        let slots = before
            .slots
            .iter()
            .enumerate()
            .map(|(index, s)| {
                s.as_ref().map(|s| api::Slot {
                    item: self.catalog.item(s.item).unwrap().key.as_ref(),
                    count: s.count,
                    has_components: s.components.is_some(),
                    stack_key: before.slots[..index]
                        .iter()
                        .position(|old| {
                            old.as_ref().is_some_and(|old| {
                                old.item == s.item && old.components == s.components
                            })
                        })
                        .unwrap_or(index) as u8,
                })
            })
            .collect::<Vec<_>>();
        let plan = self
            .definition
            .behavior
            .plan(&api::Context {
                id: snapshot.id.get(),
                tick,
                due,
                slots: &slots,
                data: &before.data,
                fuel: before.fuel,
                progress: before.progress,
            })
            .map_err(|_| EntityError::InvalidPayload)?;
        if plan.next_tick <= due || plan.data.len() > 1024 || plan.work.len() > 8 {
            return Err(EntityError::InvalidPayload);
        }
        let mut after = before.clone();
        after.data = plan.data;
        let mut transfer = None;
        for work in plan.work {
            match work {
                api::Work::Process => {
                    self.process(&mut after)?;
                    break;
                }
                api::Work::Transfer {
                    offset,
                    own_port,
                    peer_port,
                    push,
                    selection,
                } => {
                    transfer = self.transfer(
                        snapshot,
                        neighbours,
                        offset,
                        &own_port,
                        peer_port.as_deref(),
                        push,
                        &selection,
                    )?;
                    if transfer.is_some() {
                        break;
                    }
                }
            }
        }
        self.encode(&EntityPayload::new(after.clone()))
            .map_err(|_| EntityError::InvalidPayload)?;
        let anchor = snapshot.anchor().ok_or(EntityError::WrongOwnership)?;
        let cells = self.cells(anchor, &after)?;
        Ok(EntityTickPlan {
            lifecycle: Default::default(),
            payload: (after != *before).then(|| EntityPayload::new(after.clone())),
            next_tick: Some(plan.next_tick),
            anchor_update: Some(AnchorUpdate {
                anchor,
                anchor_state: cells.iter().find(|(c, _)| *c == anchor).unwrap().1,
                footprint: cells.iter().map(|(c, _)| *c).collect(),
            }),
            position: None,
            block_states: self.block_states(snapshot, before, &after)?,
            wakes: vec![],
            transfer,
        })
    }
}
