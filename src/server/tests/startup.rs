use super::*;

struct StartupProbeCodec;

impl crate::server::entities::EntityPayloadCodec for StartupProbeCodec {
    fn decode(
        &self,
        bytes: &[u8],
    ) -> Result<crate::server::entities::EntityPayload, crate::server::entities::EntityCodecError>
    {
        let [value] = bytes else {
            return Err(crate::server::entities::EntityCodecError::InvalidData);
        };
        Ok(crate::server::entities::EntityPayload::new(*value))
    }

    fn encode(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        payload
            .downcast_ref::<u8>()
            .copied()
            .map(|value| vec![value])
            .ok_or(crate::server::entities::EntityCodecError::InvalidData)
    }

    fn public_view(
        &self,
        payload: &crate::server::entities::EntityPayload,
    ) -> Result<Vec<u8>, crate::server::entities::EntityCodecError> {
        self.encode(payload)
    }
}

#[test]
fn startup_extension_registers_entity_codec_and_executes_owner_system() {
    use crate::server::entities::{EntityOwnership, EntityPayload, TickPolicy};
    use crate::server::parallel::{OwnerData, OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
    use crate::server::registry::{
        OwnerPartition, ResourceId, SystemDescriptor, SystemHandlerError, SystemId,
    };
    use crate::server::startup::StartupEntityType;

    let mut catalog = crate::content::Catalog::builtins();
    let entity_id = crate::content::EntityTypeId(70_003);
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: entity_id,
            key: "test:probe_entity".into(),
            schema_version: 1,
            schema_fingerprint: 0x5052_4f42_4500_0001,
        })
        .unwrap();
    let mut startup = ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(StartupEntityType {
        key: "test:probe_entity".into(),
        ownership: EntityOwnership::Mobile,
        tick_policy: TickPolicy::Never,
        max_payload_bytes: 1,
        codec: Arc::new(StartupProbeCodec),
        interaction_policy: None,
        tick_planner: None,
    });

    let system = SystemId::new("test:probe_system").unwrap();
    startup.register_system(
        SystemDescriptor::new(
            system.clone(),
            Phase::Simulation,
            OwnerPartition::Chunk,
            2,
            0,
        )
        .write(ResourceId::new("test:probe_state").unwrap()),
        |job: &OwnerJob| {
            let value = job
                .snapshot(job.owner())
                .and_then(|snapshot| snapshot.value::<OwnerData>())
                .and_then(|data| data.get::<u64>())
                .copied()
                .ok_or_else(|| SystemHandlerError::Rejected("missing owner state".into()))?;
            Ok(OwnerPatch::new(
                job,
                OwnerData::new(value + 1),
                PatchUsage {
                    writes: 1,
                    effects: 0,
                    estimated_bytes: std::mem::size_of::<u64>(),
                },
            ))
        },
    );
    let owner = OwnerKey::chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 });
    startup.seed_owner(system.clone(), owner, 41u64);

    let save = TestSave::new("startup-extension-live");
    let mut state = server_state_with_startup(7, save.path().to_path_buf(), 1, startup).unwrap();
    let descriptor = state.entities.types().descriptor(entity_id).unwrap();
    let payload = EntityPayload::new(7u8);
    assert_eq!(descriptor.encode_payload(&payload).unwrap(), [7]);
    assert_eq!(descriptor.public_view(&payload).unwrap(), [7]);
    assert_eq!(
        descriptor
            .decode_payload(1, &[7])
            .unwrap()
            .downcast_ref::<u8>(),
        Some(&7)
    );
    let spawn = state
        .entities
        .prepare_spawn(crate::server::entities::EntitySpawn::Mobile {
            entity_type: entity_id,
            position: [0.5, 80.0, 0.5],
            payload,
            spawn_tick: 1,
        })
        .unwrap();
    let spawned_id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    let checkpoint = crate::server::entities::encode_checkpoint(&state.entities).unwrap();
    let restored = crate::server::entities::decode_checkpoint(
        &checkpoint,
        Arc::new(state.entities.types().clone()),
    )
    .unwrap();
    assert_eq!(restored.public_view(spawned_id).unwrap().payload, [7]);
    tick_once(&mut state, TickId::new(1), Instant::now()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system, owner),
        Some((1, 42))
    );
}

#[test]
fn invalid_startup_owner_seed_does_not_create_a_world() {
    let save = TestSave::new("invalid-startup-owner");
    let mut startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    startup.seed_owner(
        crate::server::registry::SystemId::new("test:missing_system").unwrap(),
        crate::server::parallel::OwnerKey::chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 }),
        1u64,
    );
    assert_eq!(std::fs::read_dir(save.path()).unwrap().count(), 0);
    let error = server_state_with_startup(7, save.path().to_path_buf(), 1, startup)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(std::fs::read_dir(save.path()).unwrap().count(), 0);
}

#[test]
fn independent_worlds_keep_their_own_startup_catalogs() {
    let base_save = TestSave::new("base-world-catalog");
    let extended_save = TestSave::new("extended-world-catalog");
    let base = Arc::new(crate::content::Catalog::builtins());
    let mut extended = crate::content::Catalog::builtins();
    let custom_item = crate::content::ItemId(70_002);
    let texture = extended
        .block_type(crate::content::BlockTypeId(3))
        .unwrap()
        .textures
        .top;
    extended
        .register_item(crate::content::ItemDef {
            id: custom_item,
            key: "test:catalog_only".into(),
            name: "Catalog-only token".into(),
            swatch: [0.2, 0.4, 0.7, 1.0],
            texture,
            placeable: None,
            sprite: true,
        })
        .unwrap();
    let extended = Arc::new(extended);

    let base_state = server_state_with_limit_and_catalog(
        7,
        base_save.path().to_path_buf(),
        1,
        Arc::clone(&base),
    )
    .unwrap();
    let extended_state = server_state_with_limit_and_catalog(
        7,
        extended_save.path().to_path_buf(),
        1,
        Arc::clone(&extended),
    )
    .unwrap();
    assert_eq!(base_state.world.catalog().fingerprint(), base.fingerprint());
    assert_eq!(
        extended_state.world.catalog().fingerprint(),
        extended.fingerprint()
    );
    assert_ne!(base.fingerprint(), extended.fingerprint());
    assert!(base_state.world.catalog().item(custom_item).is_none());
    assert!(extended_state.world.catalog().item(custom_item).is_some());

    let mut inventory = Inventory::default();
    assert_eq!(
        inventory.insert_with_catalog(custom_item, 1, extended_state.world.catalog()),
        0
    );
    let encoded =
        InventoryStore::encode_snapshot_with_catalog(&inventory, extended_state.world.catalog())
            .unwrap();
    assert!(
        InventoryStore::decode_snapshot_with_catalog(&encoded, base_state.world.catalog()).is_err()
    );
    assert_eq!(
        InventoryStore::decode_snapshot_with_catalog(&encoded, extended_state.world.catalog())
            .unwrap(),
        inventory
    );
}

#[test]
fn missing_entity_registration_rejects_catalog_before_touching_world_files() {
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: crate::content::EntityTypeId(70_003),
            key: "test:missing_codec".into(),
            schema_version: 1,
            schema_fingerprint: 1,
        })
        .unwrap();
    let catalog = Arc::new(catalog);

    let fresh = TestSave::new("unregistered-entity-fresh");
    assert_eq!(std::fs::read_dir(fresh.path()).unwrap().count(), 0);
    let error =
        server_state_with_limit_and_catalog(7, fresh.path().to_path_buf(), 1, Arc::clone(&catalog))
            .err()
            .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(std::fs::read_dir(fresh.path()).unwrap().count(), 0);

    let existing = TestSave::new("unregistered-entity-existing");
    drop(state_for(&existing, 7));
    let before = std::fs::read(existing.path().join("content.map")).unwrap();
    let error = server_state_with_limit_and_catalog(7, existing.path().to_path_buf(), 1, catalog)
        .err()
        .unwrap();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(
        std::fs::read(existing.path().join("content.map")).unwrap(),
        before
    );
}
