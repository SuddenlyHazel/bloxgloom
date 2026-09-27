use super::*;
use crate::server::{
    entities::{EntityPayload, EntitySpawn},
    startup::ServerStartup,
};
use bloxgloom_host_api::{
    Extension, FootprintCell, Registrar, RegistrationError, anchored as api,
    entity::{Error, Payload},
};
use std::sync::Arc;

struct CodecProbe;
impl api::Behavior for CodecProbe {
    fn initialize(&self, _: [i32; 3]) -> Result<Payload, Error> {
        Ok(Payload::new(0u8))
    }
    fn encode(&self, p: &Payload) -> Result<Vec<u8>, Error> {
        Ok(vec![*p.downcast_ref::<u8>().ok_or(Error::InvalidState)?])
    }
    fn decode(&self, bytes: &[u8]) -> Result<Payload, Error> {
        // A malicious/buggy runtime update to 1 encodes, but cannot recover.
        if bytes != [0] {
            return Err(Error::InvalidState);
        }
        Ok(Payload::new(0u8))
    }
    fn public(&self, p: &Payload) -> Result<Vec<u8>, Error> {
        self.encode(p)
    }
    fn react(&self, _: &api::Context<'_>) -> Result<api::Reaction, Error> {
        Ok(api::Reaction::Keep)
    }
    fn interact(&self, _: &Payload, _: &[u8]) -> Result<Payload, Error> {
        Ok(Payload::new(1u8))
    }
}
impl Extension for CodecProbe {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        r.anchored_block_entity(api::AnchoredBlockEntity {
            entity: "test:codec_probe".into(),
            block: "bloxgloom:stone".into(),
            placement_item: "bloxgloom:stone".into(),
            anchor_state: "bloxgloom:stone".into(),
            footprint: vec![FootprintCell {
                offset: [0; 3],
                state: "bloxgloom:stone".into(),
            }],
            placement_cost: 1,
            removal_refund: 1,
            schema_version: 1,
            schema_fingerprint: 1,
            max_state_bytes: 1,
            max_public_bytes: 1,
            interval: 20,
            observe: vec![],
            interaction: vec![1],
            behavior: Arc::new(CodecProbe),
        })
    }
}

#[test]
fn unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers() {
    let path = temp_save_dir("anchored-codec-roundtrip");
    let open = || {
        crate::server::server_state_with_startup(
            7,
            path.clone(),
            1,
            ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
                .with_extension(&CodecProbe)
                .unwrap(),
        )
        .unwrap()
    };
    let mut state = open();
    let entity_type = state
        .world
        .catalog()
        .entity_type_id_by_key("test:codec_probe")
        .unwrap();
    let anchor = CellCoord::new(0, 80, 0);
    state.world.edit(0, 80, 0, crate::world::STONE).unwrap();
    let spawn = |value| EntitySpawn::Anchored {
        entity_type,
        anchor,
        anchor_state: crate::world::STONE,
        footprint: vec![anchor],
        payload: EntityPayload::new(value),
        spawn_tick: 0,
    };
    let next_id = state.durability.next_id;
    assert!(state.entities.prepare_spawn(spawn(1u8)).is_err());
    assert_eq!(state.durability.next_id, next_id);
    assert!(state.durability.pending.is_empty());
    let id = stage_entity_spawn(&mut state, spawn(0u8));
    let before = state.entities.snapshot(id).unwrap();
    assert!(
        state
            .entities
            .prepare_update(
                id,
                before.revision,
                crate::server::entities::EntityPatch {
                    payload: Some(EntityPayload::new(1u8)),
                    next_tick: None,
                    position: None
                }
            )
            .is_err()
    );
    assert!(state.durability.pending.is_empty());
    drop(state);
    let recovered = open();
    let snapshot = recovered.entities.snapshot(id).unwrap();
    assert_eq!(snapshot.private_payload.downcast_ref::<u8>(), Some(&0));
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}
