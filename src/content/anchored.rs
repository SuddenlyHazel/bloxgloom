use super::*;
use bloxgloom_host_api::{RegistrationError as ApiError, anchored::AnchoredBlockEntity};
use std::sync::Arc;

impl Catalog {
    pub(crate) fn anchored_for_state(
        &self,
        state: BlockStateId,
    ) -> Option<(EntityTypeId, &Arc<AnchoredBlockEntity>)> {
        let block = self.state(state)?.block_type;
        let id = (*self.anchored_blocks.get(block.0 as usize)?)?;
        Some((id, self.anchored_entity(id)?))
    }
    pub(crate) fn anchored_entity(&self, id: EntityTypeId) -> Option<&Arc<AnchoredBlockEntity>> {
        self.anchored_entities.get(id.0 as usize)?.as_ref()
    }
    pub(crate) fn anchored_entities(
        &self,
    ) -> impl Iterator<Item = (EntityTypeId, &Arc<AnchoredBlockEntity>)> {
        self.anchored_entities
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.as_ref().map(|d| (EntityTypeId(i as u32), d)))
    }
    pub(crate) fn register_anchored(&mut self, d: AnchoredBlockEntity) -> Result<(), ApiError> {
        d.validate()?;
        let bad = || ApiError("unresolved or incompatible anchored lifecycle".into());
        let block = self.block_by_key(&d.block).ok_or_else(bad)?;
        let anchor = self.state_by_key(&d.anchor_state).ok_or_else(bad)?;
        if !self
            .items()
            .any(|i| i.key == d.placement_item && i.placeable == Some(anchor))
            || self
                .anchored_entities()
                .any(|(_, old)| old.block == d.block)
            || d.footprint.iter().any(|c| {
                self.state_by_key(&c.state)
                    .and_then(|s| self.state(s))
                    .is_none_or(|s| s.block_type != block)
            })
        {
            return Err(bad());
        }
        // Declarative ownership/economy/read limits also participate in compatibility.
        let mut hash = 0xcbf29ce484222325u64;
        let metadata = format!(
            "anchored-v1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{:?}",
            d.schema_fingerprint,
            d.block,
            d.placement_item,
            d.anchor_state,
            d.placement_cost,
            d.removal_refund,
            d.max_state_bytes,
            d.max_public_bytes,
            d.interval,
            d.schema_version,
            d.footprint,
            d.observe,
            d.interaction
        );
        for byte in metadata.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let id = EntityTypeId(self.entities.len() as u32);
        self.register_entity_type(EntityTypeDef {
            id,
            key: d.entity.clone().into(),
            schema_version: d.schema_version,
            schema_fingerprint: hash,
        })
        .map_err(|e| ApiError(format!("anchored identity: {e:?}")))?;
        self.bind_anchored(id, Arc::new(d))
    }
    pub(crate) fn bind_anchored(
        &mut self,
        id: EntityTypeId,
        d: Arc<AnchoredBlockEntity>,
    ) -> Result<(), ApiError> {
        d.validate()?;
        let block = self
            .block_by_key(&d.block)
            .ok_or_else(|| ApiError("missing anchored block".into()))?;
        if self.entity_type(id).is_none_or(|e| e.key != d.entity)
            || self.anchored_entity(id).is_some()
        {
            return Err(ApiError("invalid anchored binding".into()));
        }
        self.anchored_entities
            .resize_with(self.entities.len(), || None);
        let action = (!d.interaction.is_empty()).then(|| bloxgloom_host_api::actions::Action {
            key: format!("{}/interact", d.entity),
            version: 1,
            label: "USE".into(),
            target: bloxgloom_host_api::actions::Target::Block(d.block.clone()),
            operation: bloxgloom_host_api::actions::Operation::EntityRequest(d.interaction.clone()),
            panel: None,
            command: None,
        });
        self.anchored_entities[id.0 as usize] = Some(d);
        self.anchored_blocks.resize(self.blocks.len(), None);
        self.anchored_blocks[block.0 as usize] = Some(id);
        if let Some(action) = action {
            self.register_action(action)?;
        }
        Ok(())
    }
}
