use super::super::transfer::{PortRoute, put, take};
use super::*;
use crate::server::voxel_view::VoxelView;
impl Adapter {
    pub(super) fn process(&self, p: &mut MachinePayload) -> Result<(), EntityError> {
        let d = self
            .definition
            .process
            .as_ref()
            .ok_or(EntityError::InvalidPayload)?;
        let input = p.slots[d.input as usize]
            .as_ref()
            .filter(|s| s.components.is_none());
        let recipe = input.and_then(|s| {
            self.recipe(Some(s.item))
                .filter(|r| s.count >= r.input_count)
        });
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
        let output = Stack::new(self.item(&recipe.output)?, recipe.output_count);
        let mut destination = p.slots[d.output as usize].clone();
        if !put(&mut destination, &output) {
            p.fuel = p.fuel.saturating_sub(1);
            return Ok(());
        }
        if let Some(fuel_slot) = d.fuel {
            if p.fuel == 0 {
                let Some(fuel) = p.slots[fuel_slot as usize]
                    .as_ref()
                    .filter(|s| s.components.is_none())
                else {
                    return Ok(());
                };
                let Some(value) = self.lookups.fuels.get(&fuel.item) else {
                    return Err(EntityError::InvalidPayload);
                };
                p.fuel = *value;
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
    #[allow(clippy::too_many_arguments)]
    fn transfer(
        &self,
        snapshot: &EntitySnapshot,
        p: &MachinePayload,
        neighbours: &EntityView,
        offset: [i32; 3],
        own_port: &str,
        peer_port: Option<&str>,
        push: bool,
    ) -> Result<Option<EntityItemTransfer>, EntityError> {
        if !api::FACES.contains(&offset) {
            return Err(EntityError::InvalidPayload);
        }
        let anchor = snapshot.anchor().ok_or(EntityError::WrongOwnership)?;
        let from = [anchor.x, anchor.y, anchor.z];
        let to = std::array::from_fn(|i| from[i].saturating_add(offset[i]));
        let peer_cell = CellCoord::new(to[0], to[1], to[2]);
        let own_index = self
            .definition
            .ports
            .iter()
            .position(|p| p.name == own_port)
            .ok_or(EntityError::InvalidPayload)? as u8;
        let Some(own) = EntityTransferPolicy::port(self, own_index, offset) else {
            return Ok(None);
        };
        let public = self
            .public_view(&EntityPayload::new(p.clone()))
            .map_err(|_| EntityError::InvalidPayload)?;
        for peer in neighbours.iter().filter(|e|matches!(&e.location,EntityLocation::Anchored {footprint,..} if footprint.contains(&peer_cell))) {
            for (peer_index,name) in neighbours.ports(peer.id).iter().enumerate() {
                if peer_port.is_some_and(|wanted|wanted!=name) {continue;}
                let Some(port)=neighbours.port(peer.id,peer_index as u8,offset.map(|v|-v)) else {continue;};
                let offers=if push {own.offers(&public)}else{port.offers(&peer.payload)};
                for mut stack in offers {stack.count=1;let fits=if push {port.accepts(&peer.payload,&stack,&self.catalog)}else{own.accepts(&public,&stack,&self.catalog)};
                    if fits {return Ok(Some(EntityItemTransfer {source:peer.id,push,item:stack.item,count:1,route:Some(if push {PortRoute {source:own_index,destination:peer_index as u8,from,to}}else{PortRoute {source:peer_index as u8,destination:own_index,from:to,to:from}})}));}
                }
            }
        }
        Ok(None)
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
            .map(|s| {
                s.as_ref().map(|s| api::Slot {
                    item: self.catalog.item(s.item).unwrap().key.as_ref(),
                    count: s.count,
                    has_components: s.components.is_some(),
                })
            })
            .collect::<Vec<_>>();
        let plan = self
            .definition
            .behavior
            .plan(&api::Context {
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
                } => {
                    transfer = self.transfer(
                        snapshot,
                        &after,
                        neighbours,
                        offset,
                        &own_port,
                        peer_port.as_deref(),
                        push,
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
