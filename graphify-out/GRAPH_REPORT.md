# Graph Report - bloxgloom  (2026-10-02)

## Corpus Check
- 1312 files · ~1,558,380 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 66 file(s) not represented in the graph (top: .wgsl 20, .mesh 15, .glb 15)

## Summary
- 16694 nodes · 39207 edges · 847 communities (535 shown, 312 thin omitted)
- Extraction: 95% EXTRACTED · 5% INFERRED · 0% AMBIGUOUS · INFERRED: 2088 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `14d07b8a`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- VoxelView
- EntityStore
- PendingWakeStore
- super
- src/preview.rs
- ScriptAnchored
- durable/actions/tests.rs
- state_for
- .bind_machine
- ScriptError
- server/runtime/tests.rs
- Complete modding implementation proposal
- actions/entity.rs
- OwnerPatch
- server/fire/tests.rs
- server/durable.rs
- InventoryProbe
- SystemId
- Rain
- PackageSnapshot
- Error
- protocol.rs
- Port<P>
- OutboundFrame
- StateKey
- WorldSnapshot
- MobileProbe
- receipts.rs
- entities/types.rs
- handler.rs
- Handler
- ui/draw.rs
- Change
- server_state_with_startup
- EntityIndexes
- server.rs
- Error
- intent.rs
- JoinApp
- journal/rotation.rs
- Resolved
- join
- BlockActionContext
- drops/planning.rs
- src/entity.rs
- Renderer
- UiLayout
- MobileEntity
- Insects
- server/effects.rs
- protocol/tests.rs
- bench.rs
- intent/tests.rs
- model_asset.rs
- Replicas
- journal.rs
- ServerStartup
- server/script.rs
- Registrar
- client/entities/tests.rs
- server/drops/tests.rs
- parallel.rs
- perf/fixture.rs
- World
- OwnerEffectPatch
- handler_declarer
- PalettedBlocks
- server/entities/tests.rs
- conflict_tests.rs
- declarer
- protocol/sounds.rs
- perf/fire.rs
- ChunkCache
- ClientMessage
- lighting.rs
- Network
- key
- reactor.rs
- HandlerRegistration
- Appearance
- src/world.rs
- InventoryStore
- entities/player.rs
- MetricsRecorder
- tcp.rs
- Declarations
- GameUi
- Diagnostics
- owner/tests.rs
- Effect
- JournalWriter
- Authored materials and effects
- CheckpointWriter
- deviceext
- EntityCheckpointMirror
- Connection
- ClientHandle
- TickId
- parse
- durable/coordinator.rs
- third_person.rs
- package/client.rs
- .place_or_interact
- join_worker.rs
- durable/state.rs
- VisualAvatar
- dispatch.rs
- ClientApp
- solver.rs
- world/terrain.rs
- State
- ComponentValue
- ChunkLoader
- PlayerRules
- script_startup/system.rs
- Session
- Item
- Clock
- DropAnimator
- entities/checkpoint.rs
- serve
- burn.rs
- TerrainReads
- StorageBlockEntity
- HarvestSnapshot
- snapshots/tests.rs
- authored/tests.rs
- RainConfig
- script/runtime.rs
- net.rs
- SunShadowQuality
- Plan: built-in/mod capability parity
- Gpu
- InventoryId
- VisualSession
- entity_recovery/tests.rs
- ClientBundle
- host-api/src/actions.rs
- startup/tests.rs
- Rng
- presentation.rs
- Behavior
- Buffer
- Session
- GenerationError
- drops/entity.rs
- MovingSpawn
- world/generation.rs
- publication/commit.rs
- .prepare_benchmark_frontier_wave
- State
- Adapter
- InventoryScreen
- render/drops.rs
- Texture
- CacheKey
- custom/shader.rs
- package
- PostProcess
- Decision
- TransportStats
- script_startup/creature.rs
- Growth foundation plan
- world/generation/tests.rs
- Service
- common.rs
- render/effects.rs
- Adapter
- view.rs
- drops/queries.rs
- client/audio/obstruction.rs
- world/tests.rs
- gameplay/decisions.rs
- item_visuals.rs
- handles.rs
- Execution
- lifecycle-fixture/src/system.rs
- moving/load.rs
- custom.rs
- init
- Scripting capabilities for mod developers
- .owner_systems
- Stack
- render/tests.rs
- effects/registered.rs
- UiRenderer
- AnchoredBlockEntity
- articulated.py
- voxel_view.rs
- validate_spawn_volume
- script_startup/gameplay.rs
- client/bundle.rs
- sealed_neighborhood
- Clock
- parallel/tests.rs
- Proposal: one coherent gameplay API
- Sender
- render/material.rs
- fields_with_command
- script/gameplay.rs
- Runtime
- thunder.rs
- UiFrame<'_>
- players/lifecycle.rs
- Update
- Adapter
- VisualFire
- cold.rs
- machine_component_tests.rs
- .compose_current_package_action_with_args
- World
- sandbox.rs
- SystemDescriptor
- neighborhood.rs
- install
- CoordinatorContext
- invoke
- MovingEntity
- Mixer
- wake.rs
- Player
- record.rs
- ScriptCreature
- Preparation
- click
- draw_image
- host-api/src/machine.rs
- position_store.rs
- creature/services.rs
- native.rs
- Context<'_>
- Composition
- inventory/container.rs
- script_startup/appearance.rs
- render/pipeline.rs
- WeatherSnapshot
- entity_sleep.rs
- KilnPayload
- Execution foundation: next implementation slices
- server/appearance.rs
- script_startup.rs
- process_movement_batch
- contact_shadow.rs
- Contributor
- request_chunk
- authored.rs
- src/composition.rs
- Slots
- Context
- client/lifecycle/tests.rs
- parse
- Runtime
- package/tests.rs
- systems/world.rs
- bake_occlusion.py
- paint_icon
- State
- LodTile
- EntityTransferPolicy
- Resources
- Gpu
- Capture
- resolve_nodes
- owner_durable/tests.rs
- system/intents.rs
- Committed
- client/world.rs
- registry/tests.rs
- Hit
- Imports
- OwnerApplyReceipt
- src/lod/tests.rs
- Chunk
- entities/motion/tests.rs
- Observations
- actions/workstation.rs
- entity_checkpoint/tests.rs
- System
- Scripting Gap Closure Plan
- ScriptMachine
- server/movement.rs
- server/drops.rs
- files.rs
- AvatarRenderer
- build_stream
- protocol/drops.rs
- EntityCodecError
- Patrol
- script_startup/gameplay/player_inventory.rs
- client
- kiln/codec.rs
- ItemIcon
- PlayerEntityStore
- character_asset/gameplay/tests.rs
- actions/command.rs
- Atmosphere
- .window_event
- time
- client/startup.rs
- Inputs
- gameplay/admin.rs
- script/capacity.rs
- EntityDependencies
- Downloads
- prepare
- lod/worker.rs
- prepare
- ModelRenderer
- .new
- BootstrapContract
- _
- Worker
- src/client/tests.rs
- view
- declarer
- io
- Catalog
- PlayerState
- ecology/tests.rs
- Mesh
- declarer
- SignalPost
- Budget
- Behavior
- scheduling_tests.rs
- .register_action
- queries/tests.rs
- AppearanceState
- .accept
- 2. Player and lifecycle hooks
- EntityError
- public_systems/motion/tests.rs
- script_startup/generation.rs
- Storage
- Declarations
- streaming/entities.rs
- script_startup/bundle.rs
- src/actions/tests.rs
- Shared<T>
- rig.rs
- tick/tests.rs
- player_services/tests.rs
- Bloxgloom interface plan
- Ignitions
- client/admin.rs
- nearest_unsent
- receive_content_manifest
- prepare
- moving/lifecycle.rs
- output.rs
- ContentManifest
- public_systems/tests.rs
- integer
- Bloxgloom
- .gameplay_entities
- drop_merge.rs
- colliders.rs
- net/tests.rs
- server/checkpoint/tests.rs
- Distant terrain LOD implementation plan
- declarer
- render/lod/tests.rs
- EventRealm
- Fixture
- widgets.rs
- durable/checkpoint.rs
- .first_solid_top
- script_startup/machine.rs
- journal/tests.rs
- scene
- coder.md
- ensure
- coder-fast.md
- .finish
- declarer
- PlayerSummary
- declarer
- .draw_node
- install_sandbox_materials
- decode
- sample
- GpuPass
- material/companions.rs
- OwnerData
- src/storage/tests.rs
- CodecProbe
- .plan
- install
- content
- resolve_player_movement
- script_startup/moving.rs
- Attempt
- MobilePages
- world_to_chunk
- Presentation
- avatars/character.rs
- prepare_recovery
- extension_tests.rs
- preview/perf.rs
- streaming.rs
- host-api/src/content.rs
- declarer
- validate
- notifications.rs
- render/effects/tests.rs
- EditGpu
- mixer/tests.rs
- Public dynamic-entity surface
- Bindings
- coordinates
- first_person/tests.rs
- publication.rs
- render
- Archived plans and audits
- .decode_reader
- package/manifest.rs
- lifecycle-fixture/src/machine.rs
- items.rs
- tests/anchored.rs
- apply_latest_controls
- .handle
- avatars/tests.rs
- server/weather/tests.rs
- visual/tests.rs
- entities/moving_tests.rs
- TileKey
- reaction_removal_tests.rs
- custom/tests.rs
- General anchored block entities
- join_lifecycle.rs
- Cross-cutting integration findings
- render_async
- quad
- Client audio foundation
- Payload
- Shared
- slots
- Control
- stack
- coder-smart.md
- Phase 8: examples, parity and integrated verification
- .local_pose
- raycast_blocks
- Agent guidance
- startup/moving/tests.rs
- manifest.json
- runtime/memory.rs
- generation
- avatars/appearance.rs
- resources.rs
- set_preview_block
- save_and_read
- Codec
- Candidate
- TickSample
- FireAnimator
- obstruction_state/tests.rs
- reviewer.md
- src/storage.rs
- prepare_recovery
- Renderer
- .install_egui
- bloxgloom
- blocks/SOURCES.md
- foliage/SOURCES.md
- EGUI-POC.md
- verdant/assets/fonts/FONT.md
- fonts/README.md
- ui-entity-actions/packages/uitarget/assets/fonts/FONT.md
- ui-target-actions/packages/uitarget/assets/fonts/FONT.md
- palette/tests.rs
- CommitAction
- decode
- declarer
- Procedural
- duration
- declarer
- visual_contracts.rs
- Ceiling
- render.rs
- tests/client.rs
- admin/tests.rs
- Startup
- model_asset/tests.rs
- Acoustics
- .validate_player_selection
- client_metadata.rs
- PlayerDecision
- Luau runtime tools
- plan_observed_request
- Larger Luau packages and independent simulation features
- reads/tests.rs
- script_startup/gameplay/entities.rs
- item_visuals/tests.rs
- Player lifecycle implementation
- create_target_pipeline
- CharacterPreview
- ui/tests.rs
- State
- rain/tests.rs
- shadow_tests.rs
- spawn.rs
- tcp/report.rs
- LifecyclePlan
- Harvest
- position_store/tests.rs
- Modding: start here
- Config
- welcome/assets/fonts/FONT.md
- mlua
- load
- client/audio/tests.rs
- inventory
- predict_player_movement_with_stance
- Storm
- authored-model/generate.py
- axis
- server/effects/tests.rs
- pack
- modding/README.md
- bounded.rs
- declarations/budget.rs
- invalid
- plan_event
- seed
- memory/tests.rs
- prepare
- banding.rs
- gameplay
- voices/tests.rs
- Authoritative moving entities
- Replace
- Cadence
- .new
- CharacterAsset
- CharacterRenderer
- Capture
- gameplay_pickup.rs
- parse
- CharacterEditor
- AUTHORED-MODELS.md
- owner_commit/tests.rs
- declarer
- hierarchy
- tests/materials.rs
- tests/appearance.rs
- presentation/observations/tests.rs
- SpawnReceipt
- DropPolicy
- face/README.md
- import_napp.py
- Wander
- outbound/tests.rs
- Resampler
- commands.rs
- client/appearance.rs
- render_sandbox_previews
- client/observations/tests.rs
- .begin_window_install
- script_startup/player.rs
- pair
- bundle_ui.rs
- complete_barrier
- PublicEntity
- WorkstationView
- actors/tests.rs
- .register_inventory_screen
- Luau VM lifetime and module state
- exposure
- DistantMarker
- script_startup/gameplay/profile_state.rs
- server/lod/tests.rs
- decode
- anchored_tests.rs
- weather/codec.rs
- Reaction
- protocol/lod.rs
- entities
- colliders/tests.rs
- sun_shadow/tests.rs
- visibility.rs
- Packaged and scripted audio
- admin/weather.rs
- Game weather foundation
- Capture
- tests/effects.rs
- render/weather/tests.rs
- EntityDefinition
- Parallax
- .sample_gameplay_look
- Receiver
- Articulated renderer performance
- mpsc
- metadata
- validate_spawn
- open
- entities/motion.rs
- decode
- src/motion/tests.rs
- write_frame
- EffectLimits
- prepare
- decode
- content/moving/tests.rs
- Definitions, assets, and composition
- session_ids/tests.rs
- setup
- world
- run
- register
- src/appearance.rs
- content/appearance.rs
- Registered inventory views and screens
- Clock
- ProfileCell
- Registered actions and composed controls
- parse
- std
- app
- Atlas
- AudioOutput
- channel
- script_startup/drop_policy.rs
- Result
- Public storage lifecycle boundary
- Sandbox rendering fixtures
- menus.rs
- vm_latency.rs
- state
- receive_result
- .sound
- world_time/tests.rs
- items/SOURCES.md
- fog.rs
- Registered anchored behavior
- ObserverRegistration
- .attempt
- crate
- Package shape
- Dynamic authored UI and input
- server/script/tests.rs
- transfer/tests.rs
- .handle
- .draw_sun_shadows
- .handle
- Authored character asset tools
- Typed client replica snapshots
- probe_request
- ScriptSystem
- passes
- terrain/tests.rs
- Registered inventory machines
- server/ecology.rs
- engine/tests.rs
- item-visuals/README.md
- render/camera/tests.rs
- .frame
- MovementState
- next_collector
- content/companions.rs

## God Nodes (most connected - your core abstractions)
1. `Result` - 2303 edges
2. `EntityError` - 237 edges
3. `SystemId` - 141 edges
4. `OwnerKey` - 140 edges
5. `StateKey` - 118 edges
6. `TickId` - 105 edges
7. `ClientMessage` - 104 edges
8. `ScriptError` - 104 edges
9. `CommitAction` - 92 edges
10. `Chunk` - 87 edges

## Surprising Connections (you probably didn't know these)
- `Libraries` --references--> `require()`  [INFERRED]
  docs/modding/RUNTIME-TOOLS.md → tools/character_assets/glb.py
- `Runtime, delivery and save compatibility` --references--> `require()`  [INFERRED]
  SCRIPTING.md → tools/character_assets/glb.py
- `F4 — New flammability meets a separate edit producer` --references--> `apply_synced_batch()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/server/durable/fire.rs
- `Controls and previews` --references--> `multiply()`  [INFERRED]
  assets/models/player/master/README.md → tools/character_assets/articulated.py
- `Appearance controls` --references--> `multiply()`  [INFERRED]
  docs/modding/AUTHORED-MODELS.md → tools/character_assets/articulated.py

## Import Cycles
- 2-file cycle: `src/server/script/runtime.rs -> src/server/script/runtime/diagnostics.rs -> src/server/script/runtime.rs`
- 2-file cycle: `src/server/perf/tcp.rs -> src/server/perf/tcp/report.rs -> src/server/perf/tcp.rs`
- 2-file cycle: `src/audio/procedural/rain.rs -> src/audio/procedural/rain/scene.rs -> src/audio/procedural/rain.rs`
- 2-file cycle: `src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server/block_actions.rs`
- 3-file cycle: `src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server/net.rs`
- 3-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server.rs`
- 3-file cycle: `src/server.rs -> src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server.rs`
- 3-file cycle: `src/server/parallel.rs -> src/server/parallel/owner_wave.rs -> src/server/registry.rs -> src/server/parallel.rs`
- 4-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server.rs`

## Communities (847 total, 312 thin omitted)

### Community 0 - "VoxelView"
Cohesion: 0.04
Nodes (27): Inventory, CapturedColumn, DropTickPlanner, AnchorEchoTick, BadTick, CounterTick, FarReadTick, PlayerEchoTick (+19 more)

### Community 1 - "EntityStore"
Cohesion: 0.06
Nodes (41): encode_allocator_value(), encode_durable_record_value(), encode_motion_value(), validate_ownership_mode(), allocator_state_key(), apply_operation_to_projection(), canonical_location(), cell_state_key() (+33 more)

### Community 2 - "PendingWakeStore"
Cohesion: 0.08
Nodes (24): crc32(), decode_owner_wake_key(), decode_wake_value(), encode_wake_value(), invalid_data(), OWNER_WAKE_DOMAIN, owner_wake_key(), OWNER_WAKE_MAGIC (+16 more)

### Community 3 - "super"
Cohesion: 0.01
Nodes (13): abandoned_result_retires_transport(), completed_result_can_still_be_cancelled(), failure_and_retry_dispatch(), CYCLE_MS, INITIAL_MS, gpu_terrain_benchmark_times_shadow_pass_without_empty_timestamp_descriptors(), progress_rotation(), Codec (+5 more)

### Community 4 - "src/preview.rs"
Cohesion: 0.05
Nodes (51): render_block_preview(), render_calibration_previews(), CLIENT_MESH_RESULT_BATCH, CLIENT_PENDING_UPLOADS, render_daylight_previews(), DropPhase, FORMAT, measure_ui_prepare() (+43 more)

### Community 6 - "durable/actions/tests.rs"
Cohesion: 0.14
Nodes (37): bin_catalog(), bin_count(), bin_startup(), BinPullTick, coordinator_drain_preserves_deferred_entity_tick_until_commit(), drive_poke_tick(), entity_and_player_views_merge_with_deterministic_order(), entity_interaction_wakes_route_to_next_tick_delivery() (+29 more)

### Community 7 - "state_for"
Cohesion: 0.11
Nodes (38): drain_durable(), drop_active_len(), drop_candidates(), drop_nearby(), reside_neighbourhood(), spawn_drop(), stage_entity_batch(), state_for() (+30 more)

### Community 9 - "ScriptError"
Cohesion: 0.12
Nodes (9): ordered(), Reader<'a>, BUDGET, exhausted(), MAX_ARTIFACTS, MAX_BYTES, Reservation, reserve() (+1 more)

### Community 10 - "server/runtime/tests.rs"
Cohesion: 0.08
Nodes (35): captured_owner_reads_share_reservations_and_fence_exclusive_waves(), rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor(), arbitrated_retry_withdraws_staged_wake_flags(), chunk_owner(), deferred_producer_waves_restage_their_durable_wakes(), dishonest_effect_accounting_is_rejected(), disjoint_staged_waves_do_not_block_each_other(), effect_consumers_cannot_gain_a_write_path() (+27 more)

### Community 11 - "Complete modding implementation proposal"
Cohesion: 0.05
Nodes (39): 10. Custom models: explicitly deferred, 11. Developer workflow and maintenance rules, 12. Implementation order and deliverables, 13. Completion and verification, 14. Continuation record — update during implementation, 1. What approval means, 2. Product outcome, 3. Starting point: preserve the useful work (+31 more)

### Community 12 - "actions/entity.rs"
Cohesion: 0.17
Nodes (18): capture_dependencies(), capture_entity_view_for_plan(), capture_tick_input(), capture_view_for_plan(), commit_tick_plan(), corrupt(), interaction_sight(), permission() (+10 more)

### Community 13 - "OwnerPatch"
Cohesion: 0.04
Nodes (25): Environment, rejected(), MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_PATCH_BYTES_PER_JOB, MAX_OWNER_PATCH_WRITES_PER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, MAX_OWNER_WAVE_PATCH_WRITES, OwnerJob (+17 more)

### Community 14 - "server/fire/tests.rs"
Cohesion: 0.04
Nodes (55): key_bytes(), cursor_key(), cursor_lane(), FireRuntime, source_transaction(), SourceEncodeInput, FIRE_DELIVERY_SYSTEM_ID, FIRE_LANES (+47 more)

### Community 15 - "server/durable.rs"
Cohesion: 0.05
Nodes (16): Durability, BlockDelta, CHECKPOINT_QUEUE_CAPACITY, CHECKPOINT_WORKERS, DirtyCheckpoint, Durability, FireCheckpointBatch, MAX_DEFERRED_DURABLE_ACTIONS (+8 more)

### Community 16 - "InventoryProbe"
Cohesion: 0.06
Nodes (16): InventoryProbe, external_item_action_composed_control_receipt_duplicate_stale_and_restart(), send(), send_with_session(), connect(), counter_total(), edit(), FIRST (+8 more)

### Community 18 - "SystemId"
Cohesion: 0.04
Nodes (45): ChunkKey, OwnerKey, SystemId, crc32(), decode_cell_value(), decode_cursor_value(), decode_owner_cursor_key(), decode_owner_state_key() (+37 more)

### Community 19 - "Rain"
Cohesion: 0.05
Nodes (22): ImpactProfile, Biquad, Mode, band_next(), Bed, BED_BANDS, custom_surface(), diameter() (+14 more)

### Community 20 - "PackageSnapshot"
Cohesion: 0.07
Nodes (6): authored_drop_animation_negotiates_and_default_keeps_old_bundle(), drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle(), item_sprite_option_is_negotiated_and_omission_preserves_default_identity(), error(), Package, PackageSnapshot

### Community 21 - "Error"
Cohesion: 0.05
Nodes (9): Block, cell_random(), WorldTime, Context, Context<'a>, DropSpawn, Error, Plan (+1 more)

### Community 22 - "protocol.rs"
Cohesion: 0.07
Nodes (35): BLOCK_COUNT, Cursor, Cursor<'a>, DroppedItem, component_drop_snapshot_and_pickup_pages_fit_frames_without_truncating_payloads(), component_drops_and_pickups_round_trip_exact_catalog_validated_stack_and_lengths(), drop(), frame() (+27 more)

### Community 23 - "Port<P>"
Cohesion: 0.15
Nodes (4): Interaction, Port, Port<P>, public_slots()

### Community 24 - "OutboundFrame"
Cohesion: 0.07
Nodes (13): Identity, SharedParts, ClientQueueTelemetry, OUTBOUND_AGGREGATE_BYTE_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundError, OutboundFrame (+5 more)

### Community 25 - "StateKey"
Cohesion: 0.08
Nodes (28): decode_snapshot(), encode_snapshot(), FILE_NAME, MAGIC, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_KEYS, read_snapshot_file(), TEMP_SEQUENCE (+20 more)

### Community 26 - "WorldSnapshot"
Cohesion: 0.06
Nodes (10): block(), combine_entities(), dispatch_neighbors(), error(), OperationInput, Participants, plan_removals(), plan_with_lifecycles() (+2 more)

### Community 27 - "MobileProbe"
Cohesion: 0.07
Nodes (10): MobileProbe, NetworkedVisualProbe, console_commands_reject_non_admin_and_recover_grant_over_nonblocking_listener(), creature_probe(), external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener(), mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart(), mixed_work(), response_samples() (+2 more)

### Community 28 - "receipts.rs"
Cohesion: 0.08
Nodes (23): Admission, checksum(), invalid(), MAGIC, MAX_PAYLOAD, MAX_REASON, MAX_SNAPSHOT, ReceiptEvent (+15 more)

### Community 29 - "entities/types.rs"
Cohesion: 0.07
Nodes (18): validate_location_owner(), AnchorUpdate, CellCoord, EntityId, EntityLocation, EntityOwner, EntityOwnership, EntityView (+10 more)

### Community 30 - "handler.rs"
Cohesion: 0.07
Nodes (16): FireFrontier, FireDeliveryInput, FireDeliveryPatch, FireOwnerInput, FireOwnerPatch, MAX_DELIVERIES_PER_OWNER, MAX_DUE_CELLS_PER_OWNER, MAX_IGNITIONS_PER_OWNER (+8 more)

### Community 31 - "Handler"
Cohesion: 0.08
Nodes (23): Handler, action_and_tick_expand_secondary_cell_once_and_refund_final_inventory_on_restart(), barrier(), commit_command(), Destroy, GroundRemoved, Neighbor, open_anchor() (+15 more)

### Community 32 - "ui/draw.rs"
Cohesion: 0.10
Nodes (23): UiBuilder<'_>, UiBuilder<'_>, EDGE, FONT_HEIGHT, FONT_WIDTH, GOLD, inset(), item_color() (+15 more)

### Community 33 - "Change"
Cohesion: 0.04
Nodes (20): Durability, CommitBarrier, Change, owner_cursor_key(), arbitrate_key_sets(), build_owner_writes_parallel(), canonical_key_set(), OwnerCommit (+12 more)

### Community 34 - "server_state_with_startup"
Cohesion: 0.12
Nodes (39): tick_once(), server_state_with_startup(), durable_counter_startup(), durable_pair_startup(), durable_twin_startup(), entity_and_owner_state_commit_as_one_atomic_record(), external_neighbor_reads_defer_until_all_chunks_arrive_and_fence_adjacent_edits(), external_owner_can_durably_wake_another_owner_after_restart() (+31 more)

### Community 35 - "EntityIndexes"
Cohesion: 0.11
Nodes (14): _entity_cell_key_round_trip(), Bucket, ChunkPage, decode_cell_key(), decode_cell_owner(), decode_chunk_key(), encode_cell_key(), encode_cell_owner() (+6 more)

### Community 36 - "server.rs"
Cohesion: 0.04
Nodes (35): catalog_with_extension(), Client, DEFAULT_CLIENTS, DEFAULT_VIEW, spawn_effects(), EDIT_REACH, handle_message(), INPUT_CAPACITY (+27 more)

### Community 37 - "Error"
Cohesion: 0.07
Nodes (9): cached_profile_inventory_rechecks_each_handler_authority_and_latches_denial(), handler_random_is_stable_per_seed_cell_and_registration(), ignored_failures_cannot_publish_partial_operations(), Inventories, moving_spawn_references_are_local_and_never_predict_durable_ids(), private_entity_overlay_does_not_authorize_the_next_decision_owner(), staged_motion_rechecks_owner_coalesces_fields_and_rejects_caught_errors(), transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides() (+1 more)

### Community 38 - "intent.rs"
Cohesion: 0.10
Nodes (18): blocked(), decode(), decode_key(), encode(), is_key(), key(), Reader, Reader<'a> (+10 more)

### Community 40 - "journal/rotation.rs"
Cohesion: 0.13
Nodes (36): invalid_data(), Base, BASE_FORMAT_VERSION, BASE_FORMAT_VERSION_LEGACY, BASE_MAGIC, BASE_MAX_BYTES, base_path(), check_checksum() (+28 more)

### Community 42 - "join"
Cohesion: 0.13
Nodes (31): failed_startup_queue_does_not_register_a_ghost_profile(), full_outbound_queue_disconnects_only_the_slow_client(), joined_players_spawn_above_solid_terrain_with_headroom(), joins_find_lower_safe_surface_after_origin_support_is_mined(), multiple_clients_receive_edit_delta_then_resync_snapshot_in_order(), remote_player_spawn_move_and_leave_publish_ordered_entity_changes(), startup_and_live_spawn_can_use_negative_ground_after_excavation(), streamed_interest_pins_release_on_resync_and_disconnect() (+23 more)

### Community 43 - "BlockActionContext"
Cohesion: 0.05
Nodes (26): BlockActionContext, BlockActionHooks, BlockActionRegistry, BlockActionRegistryBuilder, BlockActionRegistryBuilder<'a>, BlockCommitBuilder, invoke_hook(), MAX_BLOCK_ACTION_HANDLERS (+18 more)

### Community 44 - "drops/planning.rs"
Cohesion: 0.13
Nodes (25): merge_target(), plan_error(), plan_expired(), plan_spawn_stack(), plan_spawns(), plan_spawns_with_extra(), plan_stack_spawns(), plan_stack_spawns_with_extra() (+17 more)

### Community 45 - "src/entity.rs"
Cohesion: 0.08
Nodes (14): Animation, Behavior, Body, Context, Cuboid, Error, Lifecycle, Movement (+6 more)

### Community 46 - "Renderer"
Cohesion: 0.04
Nodes (8): next_upload_index(), order_pending_mesh(), Renderer, RendererError, RenderStats, target_outline_vertices(), urgent_mesh_reorders_existing_pending_chunk_without_duplication(), create_depth()

### Community 47 - "UiLayout"
Cohesion: 0.09
Nodes (12): InventorySearch, search_rect(), join_action_rect(), centered_panel(), effective_ui_scale(), HitRect, UiLayout, JoinProgress (+4 more)

### Community 48 - "MobileEntity"
Cohesion: 0.11
Nodes (7): MobileEntity, Spawn, definition(), KEY, State, definition(), Catalog

### Community 49 - "Insects"
Cohesion: 0.08
Nodes (14): Command, Controls, MAX_CLIP_VOICES, volume(), Insects, place(), rest(), sources() (+6 more)

### Community 50 - "server/effects.rs"
Cohesion: 0.18
Nodes (15): block_change_owners(), boundary_coordinates(), CellCoord, Effect, EffectBatch, EffectBuffer, EffectBufferError, EffectEnvelope (+7 more)

### Community 51 - "protocol/tests.rs"
Cohesion: 0.12
Nodes (29): read_server(), action_receipts_round_trip_and_reject_invalid_ids(), admin_grant_wire_round_trips_and_rejects_invalid_counts(), catalog_with_many_states(), character_selection_is_session_scoped_and_bounded_on_wire(), client_messages_round_trip(), committed_action_spawn_mappings_roundtrip_and_reject_invalid_ordinals(), committed_fire_cues_round_trip_with_a_strict_cell_bound() (+21 more)

### Community 52 - "bench.rs"
Cohesion: 0.08
Nodes (17): FireApplyTimings, ACTIVE_CHUNKS, benchmark_cpu(), BenchSave, cell_index(), CHUNKS_X, CHUNKS_Z, FireCpuPhaseReport (+9 more)

### Community 53 - "intent/tests.rs"
Cohesion: 0.12
Nodes (32): durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans(), durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation(), durable_intent_bootstrap_destination_conflict_retries_one_atomic_record(), durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required(), durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems(), cell(), chunk(), durable_intent_bootstrap_chain_gates_creation_retry_forwarding_and_restart() (+24 more)

### Community 54 - "model_asset.rs"
Cohesion: 0.09
Nodes (12): load(), array(), Binding, Image, load(), Material, MAX_BYTES, Model (+4 more)

### Community 55 - "Replicas"
Cohesion: 0.08
Nodes (15): Interaction, presentation and durability closure, Assembly, player_adapter(), kiln_adapter(), MAX_CHUNK_ENTITY_BYTES, MAX_CLIENT_ENTITY_BYTES, MAX_PENDING_BYTES, MAX_PENDING_COMMITS (+7 more)

### Community 56 - "journal.rs"
Cohesion: 0.04
Nodes (33): CLOCK_DOMAIN, clock_key(), crc32(), decode_transaction(), encode_frame(), frame_checksum(), frame_len(), invalid_data_owned() (+25 more)

### Community 57 - "ServerStartup"
Cohesion: 0.11
Nodes (4): entity_error(), ServerStartup, StartupEntityType, StartupOwnerCodec

### Community 58 - "server/script.rs"
Cohesion: 0.10
Nodes (11): Invocation, Limits, Output, Program, Request, run(), run_with(), ScriptFailure (+3 more)

### Community 59 - "Registrar"
Cohesion: 0.03
Nodes (42): Bundle, CubeBlock, Extension, Registrar, RegistrationError, CHIP, Content, LAMP (+34 more)

### Community 60 - "client/entities/tests.rs"
Cohesion: 0.10
Nodes (32): anchored_presentation_window_is_separate_scoped_and_sorted(), mossbun_adapter_uses_the_negotiated_catalog_assignment(), mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction(), presentation_entity_window_is_ordered_and_explicitly_bounded(), PendingSnapshot, accept(), block_and_entity_changes_wait_for_whole_cross_chunk_commit(), checksum_conflict_and_revision_gap_request_resync_without_partial_install() (+24 more)

### Community 61 - "server/drops/tests.rs"
Cohesion: 0.11
Nodes (30): active_len(), apply_expired(), assert_store_consistent(), drop_world(), drop_world_in(), DropWorld, insert_entry(), item() (+22 more)

### Community 62 - "parallel.rs"
Cohesion: 0.07
Nodes (27): BarrierError, BatchId, CancelError, CancellationToken, execute_task(), ExecutorConfigError, JobCompletion, JobKey (+19 more)

### Community 63 - "perf/fixture.rs"
Cohesion: 0.05
Nodes (36): ready(), Reset, teleport(), ACTION_INTERVAL, add_clients_and_seed_drops(), DIRT_ITEM, drain_outbound(), DrainTotals (+28 more)

### Community 64 - "World"
Cohesion: 0.09
Nodes (6): ChunkKey, ChunkReadStamp, EditBasis, LoadedChunk, PreparedEdit, World

### Community 65 - "OwnerEffectPatch"
Cohesion: 0.09
Nodes (6): BlockEdit, EditCause, blocked(), EmittedOwnerEffect, OwnerEffectPatch, route_and_consume()

### Community 68 - "PalettedBlocks"
Cohesion: 0.10
Nodes (6): LocalIndex, PalettedBlocks, PaletteView, set_palette_cell(), u16, u8

### Community 69 - "server/entities/tests.rs"
Cohesion: 0.10
Nodes (32): decode_checkpoint(), encode_checkpoint(), a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema(), anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically(), AppearanceCodec, checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types(), delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion(), DROP_TYPE (+24 more)

### Community 70 - "conflict_tests.rs"
Cohesion: 0.13
Nodes (24): action(), disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply(), drop_merge_absence_is_fenced_against_same_owner_motion_into_range(), hold(), neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders(), overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership(), plan(), PROBE (+16 more)

### Community 72 - "protocol/sounds.rs"
Cohesion: 0.08
Nodes (17): controls(), Event, identifier(), key(), Kind, position(), State, Voice (+9 more)

### Community 73 - "perf/fire.rs"
Cohesion: 0.14
Nodes (19): ACTIVE_CHUNKS, CHUNKS_X, CHUNKS_Z, drain_durable(), ensure_resident(), fixture_action(), FOREST_BATCH_CHUNKS, forest_hash() (+11 more)

### Community 74 - "ChunkCache"
Cohesion: 0.11
Nodes (3): CacheEntry, ChunkCache, OwnerState

### Community 75 - "ClientMessage"
Cohesion: 0.06
Nodes (6): PackageActionProbe, PackageActionProbe, ClientMessage, fixture(), luau_player_appearance_is_authorized_rollback_safe_peer_replicated_and_saved(), Peer

### Community 76 - "lighting.rs"
Cohesion: 0.20
Nodes (9): build_bounce(), index(), is_opaque(), LightField, MAX_LIGHT, PLANE, propagate(), SIDE (+1 more)

### Community 77 - "Network"
Cohesion: 0.08
Nodes (10): ConfigWriter, connect_bundle_probe(), connect_catalog_probe(), connect_inventory_probe(), connect_ui_probe(), connect_visual_probe(), Incoming, Network (+2 more)

### Community 78 - "key"
Cohesion: 0.28
Nodes (14): animated_item_bundle_rejects_invalid_and_noncanonical_motion(), canonical_order_dependency_identity_and_count_bounds_are_verified(), decoder_rejects_server_classification_and_oversized_payloads_before_copying(), header(), key(), material_effect_and_ui_assets_coexist_in_canonical_bundle(), metadata_validates_namespace_capability_shape_and_limits_before_compilation(), package() (+6 more)

### Community 79 - "reactor.rs"
Cohesion: 0.12
Nodes (16): ACCEPT_BUDGET, has_admission_capacity(), has_admission_capacity_with_limit(), INVENTORY_WORKERS, IO_POLL_TIMEOUT, LISTENER_KEY, MAX_PENDING_LEAVES, READ_BUDGET (+8 more)

### Community 80 - "HandlerRegistration"
Cohesion: 0.13
Nodes (4): Event, EventKind, HandlerRegistration, RemovalCause

### Community 81 - "Appearance"
Cohesion: 0.25
Nodes (3): Appearance, MAX_ADDITIONS, MODEL

### Community 82 - "src/world.rs"
Cohesion: 0.08
Nodes (31): AIR, BEDROCK_Y, BLUE_FLOWER, CHUNK_SIZE, CHUNK_VOLUME, DIRT, FERN, GLOWSTONE (+23 more)

### Community 83 - "InventoryStore"
Cohesion: 0.06
Nodes (17): checksum(), HEADER_LEN, invalid(), InventoryStore, MAGIC, MAX_LEN, MIN_LEN, SLOT_FIXED_LEN (+9 more)

### Community 84 - "entities/player.rs"
Cohesion: 0.16
Nodes (8): MAX_PLAYER_ENTITY_PAYLOAD_BYTES, MAX_SESSION_PLAYER_ENTITIES, PLAYER_ENTITY_TYPE, player_public_payload_codec_is_fixed_size(), player_type_registers_only_against_catalogued_identity(), PlayerEntityPayload, PlayerPayloadCodec, register_player_entity_type()

### Community 85 - "MetricsRecorder"
Cohesion: 0.13
Nodes (13): duration_nanos(), EVENT_LATENCY_STREAMS, LatencyEvent, LatencyRing, Metric, MetricsRecorder, MotionSample, nearest_rank() (+5 more)

### Community 86 - "tcp.rs"
Cohesion: 0.10
Nodes (15): drive(), exercise(), movement(), open_nuisance_peers(), PROFILE_BASE, run(), SEED, seed_inventories() (+7 more)

### Community 87 - "Declarations"
Cohesion: 0.08
Nodes (6): declaration_key(), Declarations, invoke(), PackageTexture, Pending, text()

### Community 88 - "GameUi"
Cohesion: 0.08
Nodes (5): draw_screen(), DrawTarget, GameUi, Intent, themed_context()

### Community 89 - "Diagnostics"
Cohesion: 0.13
Nodes (12): Buffer, Diagnostics, encode_fields(), full(), invalid(), MAX_BYTES, MAX_MESSAGE, MAX_RECORDS (+4 more)

### Community 90 - "owner/tests.rs"
Cohesion: 0.10
Nodes (19): Behavior, CELL, drop_count(), empty_action(), Fixture, Harvest, KEY, Marker (+11 more)

### Community 91 - "Effect"
Cohesion: 0.11
Nodes (3): Effect, target_sizes(), targets_remain_bounded_at_large_and_tiny_viewports()

### Community 92 - "JournalWriter"
Cohesion: 0.06
Nodes (8): CommitReceipt, Journal, Journal, JournalWriter, Request, RotateError, RotationReceipt, WriterCommand

### Community 93 - "Authored materials and effects"
Cohesion: 0.40
Nodes (5): Authored materials and effects, Effect contract 2, Material contract 2, Preparation and compatibility, Typed values from Luau

### Community 94 - "CheckpointWriter"
Cohesion: 0.09
Nodes (7): checkpoint_shard(), checkpoint_worker(), CheckpointJob, CheckpointReceipt, CheckpointSubmitError, CheckpointWriter, panic_message()

### Community 95 - "deviceext"
Cohesion: 0.08
Nodes (17): gpu_shadow_only_darkens_lit_visible_ground_and_fades_at_night(), render(), SIDE, calibrated_lighting_fixture_validates_without_gpu(), FIXTURE, gpu_daylight_preserves_palette_caves_and_unoccluded_direct_light(), source(), CASES (+9 more)

### Community 96 - "EntityCheckpointMirror"
Cohesion: 0.10
Nodes (10): CheckpointReceipt, CheckpointTicket, Command, EntityCheckpointMirror, Event, MAX_MIRROR_ADMISSIONS, MirrorMetrics, MirrorPermit (+2 more)

### Community 97 - "Connection"
Cohesion: 0.11
Nodes (9): JoinReply, JoinResponse, Connection, PendingWrite, PendingWriteKind, Phase, PendingLeave, JoinGuard (+1 more)

### Community 98 - "ClientHandle"
Cohesion: 0.12
Nodes (9): ActionKind, active_slow_peer(), ClientHandle, ClientStats, handshake(), PendingAction, read_until_stop(), Ready (+1 more)

### Community 99 - "TickId"
Cohesion: 0.09
Nodes (23): Entities, player behavior, world simulation and generation, apply_synced_batch(), Durability, run_delivery(), run_source(), stage_gameplay_burn(), stage_transactions(), stage_wave() (+15 more)

### Community 100 - "parse"
Cohesion: 0.25
Nodes (11): boolean(), declarer(), dense(), field(), model(), number(), optional_number(), owned() (+3 more)

### Community 101 - "durable/coordinator.rs"
Cohesion: 0.10
Nodes (29): batchable_motion(), cancel_prepared_entities(), command_action_id(), defer_action(), durable_request_profile(), fail_if_durability_failed(), fatal_stage_error(), finish_noncommand_request() (+21 more)

### Community 102 - "third_person.rs"
Cohesion: 0.13
Nodes (8): avatar(), GameplayPose, prepare(), Shot, DISTANCE, intersection(), Perspective, RADIUS

### Community 103 - "package/client.rs"
Cohesion: 0.06
Nodes (31): ANIMATED_MAGIC, APPEARANCE_MAGIC, APPEARANCE_POLICY_MAGIC, BLOCK_OPTIONS_MAGIC, BLOCK_STATES_MAGIC, ClientSide, ClientSource, COMPONENTS_MAGIC (+23 more)

### Community 106 - "durable/state.rs"
Cohesion: 0.11
Nodes (12): stage(), touches_anchor(), open(), action_changes(), chunk_state_key(), decode_chunk_key(), decode_profile_key(), encode_action_receipt() (+4 more)

### Community 107 - "VisualAvatar"
Cohesion: 0.07
Nodes (16): ActorAnimator, DELAY, Track, Sample, STEP, Track, Motion, avatar() (+8 more)

### Community 108 - "dispatch.rs"
Cohesion: 0.15
Nodes (16): apply(), disconnect(), prepare(), Prepared, publish(), component_pickup_fanout_pages_all_stacks_after_inventory_with_exact_payloads(), effect(), fire_cue_is_dropped_for_backlogged_client_without_disconnect() (+8 more)

### Community 109 - "ClientApp"
Cohesion: 0.05
Nodes (20): action_id(), ActionTracker, ClientApp, command_action_id(), digit_slot(), edit_for_hit(), edit_for_hit_with_catalog(), escape_screen() (+12 more)

### Community 110 - "solver.rs"
Cohesion: 0.22
Nodes (23): Blocked, Body, cap_speed(), check_bounds(), Collider, Contact, ContactMemory, ContactPolicy (+15 more)

### Community 111 - "world/terrain.rs"
Cohesion: 0.15
Nodes (31): Biome, collapse_surface(), Column, decorate_chunk(), generate_blocks(), generated_block(), generated_block_in_column(), generated_block_with_pattern() (+23 more)

### Community 112 - "State"
Cohesion: 0.06
Nodes (7): SceneSampler, MAX_RAIN_TILES, RAIN_MATERIALS, RainScene, RainTile, State, volumes()

### Community 113 - "ComponentValue"
Cohesion: 0.10
Nodes (17): ComponentMatch, ComponentOutput, ComponentValue, value_bytes(), Input, Output, StackValue, Transformation (+9 more)

### Community 114 - "ChunkLoader"
Cohesion: 0.11
Nodes (9): ChunkLoader, ChunkLoadResult, ChunkLoadTicket, Job, RequestError, RequestStatus, stop_workers(), WORKER_COUNT (+1 more)

### Community 115 - "PlayerRules"
Cohesion: 0.13
Nodes (9): Body, BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_RULES, BUILTIN_SPAWN, InvalidPlayerRules, MotionRates, PlayerRules (+1 more)

### Community 116 - "script_startup/system.rs"
Cohesion: 0.16
Nodes (21): commit(), Fixture, KEY, luau_burn_owner_uses_host_removal_semantics_and_persists_receipt(), luau_owner_after_dependencies_are_resolved_before_save_creation(), luau_owner_block_info_uses_captured_public_fields_and_restarts(), luau_owner_caught_invalid_entity_change_rejects_whole_wave(), luau_owner_drop_creation_shares_receipt_and_restarts() (+13 more)

### Community 118 - "Item"
Cohesion: 0.05
Nodes (12): Components, Item, CounterInteract, KilnInteractionPolicy, EntityInteractionPolicy, EntityPayloadCodec, EntityTypeDescriptor, EntityTypeRegistration (+4 more)

### Community 119 - "Clock"
Cohesion: 0.10
Nodes (7): advance(), Capture, Clock, DOMAIN, publish(), ReadStamp, state_key()

### Community 120 - "DropAnimator"
Cohesion: 0.28
Nodes (4): DropAnimator, live_visual(), PickupFlight, POSITION_BLEND

### Community 121 - "entities/checkpoint.rs"
Cohesion: 0.12
Nodes (14): CHECKPOINT_NAME, checkpoint_rejects_bad_magic_version_and_checksum(), checkpoint_write_read_is_atomic_and_bounded(), crash_left_temporary_checkpoint_fails_closed(), DIRECTORY_NAME, entity_invalid_data(), EntityCheckpointStore, invalid_data() (+6 more)

### Community 122 - "serve"
Cohesion: 0.04
Nodes (55): luau_action_block_targets_keep_real_reach_sight_and_identity_checks(), luau_creature_replaces_itself_with_another_authored_type_over_real_listener(), mod_admin_grant_requires_server_identity_and_replays_once_over_listener(), mod_admin_spawn_and_drop_share_one_allocator_and_restart(), connect(), package(), package_cube_flags_are_frozen_and_old_declaration_keeps_defaults(), package_cube_joins_places_and_recovers_with_identical_session_catalog() (+47 more)

### Community 123 - "burn.rs"
Cohesion: 0.11
Nodes (11): assert_burned(), assert_uncommitted(), Burn, burn_startup(), BurnGrass, definition(), destination(), plant_burn() (+3 more)

### Community 124 - "TerrainReads"
Cohesion: 0.09
Nodes (8): TerrainReads, anchored(), nearby(), project(), read(), state(), validate_owner(), validate_state()

### Community 125 - "StorageBlockEntity"
Cohesion: 0.06
Nodes (21): FootprintCell, MAX_FOOTPRINT, MAX_STORAGE_SLOTS, PlacementContext, PlaceStorage, RemovalContext, RemoveStorage, StorageBlockEntity (+13 more)

### Community 126 - "HarvestSnapshot"
Cohesion: 0.12
Nodes (4): flower_harvests_itself_and_grass_and_leaves_have_distinct_loot(), harvest(), harvest_with_catalog(), HarvestSnapshot

### Community 127 - "snapshots/tests.rs"
Cohesion: 0.09
Nodes (26): apply(), dispatch(), finish(), MAX_SNAPSHOT_JOBS, Prepared, publish(), Selection, Target (+18 more)

### Community 128 - "authored/tests.rs"
Cohesion: 0.05
Nodes (47): combined_mod_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS, farming_scale_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS (+39 more)

### Community 129 - "RainConfig"
Cohesion: 0.08
Nodes (13): Advanced, default_insect_gain(), default_wind_gain(), RainConfig, Surface, SurfaceMode, audition(), draw() (+5 more)

### Community 130 - "script/runtime.rs"
Cohesion: 0.10
Nodes (12): begin(), MAX_BYTES, MAX_ENTRIES, create(), reseed(), Seed, author_reseeding_keeps_standard_math_random_semantics(), diagnostic_encoding_rejects_nested_fields_without_executing_metamethods() (+4 more)

### Community 131 - "net.rs"
Cohesion: 0.08
Nodes (17): BundleHandshake, BUNDLE_TIMEOUT, ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT, JoinCleanup, bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline(), commands_received_after_content_ready_wait_for_join_completion() (+9 more)

### Community 132 - "SunShadowQuality"
Cohesion: 0.12
Nodes (12): SunShadowQuality, camera_layout(), depth(), depth_state(), fallback_camera_group(), group(), Projection, sampler() (+4 more)

### Community 133 - "Plan: built-in/mod capability parity"
Cohesion: 0.08
Nodes (24): 1. Establish the boundary and parity inventory, 2. Complete the container/block-entity vertical slice, 3. Complete dynamic entities and presentation, 4. Close remaining gameplay and world surfaces, 5. Prove integration outside engine internals, Adopted direction, Client presentation and resources, Completed task: player-response path hardening (+16 more)

### Community 134 - "Gpu"
Cohesion: 0.06
Nodes (7): COVERAGE_SLOTS, entry(), Gpu, MAX_BYTES, MAX_PENDING, select_ready(), Tile

### Community 135 - "InventoryId"
Cohesion: 0.20
Nodes (7): Components, Context<'_>, InventoryId, PickupTransfer, PickupTransfer<'a>, Slot, Stack

### Community 136 - "VisualSession"
Cohesion: 0.12
Nodes (3): Script, PendingBatch, VisualSession

### Community 137 - "entity_recovery/tests.rs"
Cohesion: 0.16
Nodes (12): checkpointed_motion_ahead_of_wal_fence_survives_recovery(), fixture(), lagging_checkpoint_replays_later_wal_transfer(), MOBILE_TYPE, NEXT_DIR, one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt(), position(), same_revision_conflicting_checkpoint_motion_fails_closed() (+4 more)

### Community 139 - "host-api/src/actions.rs"
Cohesion: 0.10
Nodes (18): Action, key(), MAX_ACTIONS, MAX_INTERACTION_PAYLOAD, MAX_REQUEST_ARGUMENTS, MAX_TARGET_ACTIONS, MAX_WIDGETS, Operation (+10 more)

### Community 140 - "startup/tests.rs"
Cohesion: 0.16
Nodes (19): block_auto_items_cannot_bypass_total_item_capacity(), caught_startup_execution_limit_cannot_publish_and_worker_recovers(), content_capacity_admits_full_block_item_texture_targets(), content_capacity_fixture(), content_capacity_max_plus_one_errors_survive_pcall_with_key_and_usage(), duplicate_keys_and_caught_declaration_errors_reject_all_startup(), Fixture, generator_installation_fixture() (+11 more)

### Community 141 - "Rng"
Cohesion: 0.06
Nodes (13): Rng, SAMPLE_RATE, Cicada, Cicadas, Song, Cricket, Crickets, Oscillator (+5 more)

### Community 142 - "presentation.rs"
Cohesion: 0.14
Nodes (15): bounded_float(), Command, command_entity(), display_text(), EntityView, invalid(), optional_bounded_float(), Reply (+7 more)

### Community 143 - "Behavior"
Cohesion: 0.19
Nodes (5): Behavior, Cell, Context, interaction_request(), RemovalCause

### Community 144 - "Buffer"
Cohesion: 0.18
Nodes (4): Buffer, capture(), default_filter_preserves_structured_game_events_and_flushes_final_errors(), scoped_filters_and_invalid_filter_fallback_work_without_global_state()

### Community 146 - "GenerationError"
Cohesion: 0.09
Nodes (10): CHUNK_SIZE, Context, GenerationError, in_world_bounds(), MAX_WRITES, mix(), Output, Registration (+2 more)

### Community 147 - "drops/entity.rs"
Cohesion: 0.12
Nodes (7): DROP_ENTITY_TYPE, DROP_PAYLOAD_FIXED_BYTES, DropEntityPayload, DropPayloadCodec, MAX_DROP_ENTITY_PAYLOAD_BYTES, register_entity_type(), drop_entity_registration_is_catalog_linked_and_mobile()

### Community 148 - "MovingSpawn"
Cohesion: 0.12
Nodes (13): apply(), Context<'_>, MotionChange, MotionCommand, MovingSpawn, SpawnReference, MotionContact, apply_command() (+5 more)

### Community 149 - "world/generation.rs"
Cohesion: 0.10
Nodes (17): apply(), Builtin, BUILTIN_SAMPLES, builtin_state_key(), BuiltinSamples, compose(), generate_chunk(), generate_chunk_with_contributors() (+9 more)

### Community 150 - "publication/commit.rs"
Cohesion: 0.25
Nodes (10): add_remove(), add_upsert(), collect(), CommitChanges, CommitPlan, fanout_bound(), for_client(), KeyChanges (+2 more)

### Community 152 - "State"
Cohesion: 0.09
Nodes (6): MAX_REQUESTS, MAX_TILES, neighboring(), Request, RETRY, State

### Community 153 - "Adapter"
Cohesion: 0.13
Nodes (5): Adapter, component_matches(), Lookups, MachinePayload, register()

### Community 154 - "InventoryScreen"
Cohesion: 0.18
Nodes (6): InventoryScreen, MAX_SLOTS, MAX_STATUS_FIELDS, SlotGroup, StatusField, StatusFormat

### Community 155 - "render/drops.rs"
Cohesion: 0.15
Nodes (17): block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting(), DropMeshes, emit_cutout_drop(), flower_pickup_uses_cutout_crosses_instead_of_cube_faces(), grass_side_band_is_at_the_top_on_both_side_axes(), is_sprite_item(), MAX_CUTOUT_INDEX_BYTES, MAX_CUTOUT_VERTEX_BYTES (+9 more)

### Community 156 - "Texture"
Cohesion: 0.15
Nodes (5): Texture, Catalog, error(), texture_definition(), validate_display()

### Community 157 - "CacheKey"
Cohesion: 0.07
Nodes (30): CacheKey, anchored_client_artifact_preserves_full_native_contract_and_catalog_identity(), anchored_client_artifact_rejects_storage_and_machine_ownership_collisions(), anchored_client_artifact_rejects_unresolved_refs_truncation_and_nested_wrappers(), artifact(), artifact_on(), base(), declaration() (+22 more)

### Community 158 - "custom/shader.rs"
Cohesion: 0.09
Nodes (11): validate(), compose(), STUBS, TYPES, validate(), validate(), compose(), STUBS (+3 more)

### Community 159 - "package"
Cohesion: 0.28
Nodes (8): luau_anchored_callbacks_accept_full_registered_binary_limits_and_immutable_inputs(), luau_anchored_rejects_failed_validation_oversized_public_and_ambiguous_reaction(), luau_anchored_invalid_interaction_and_excess_refund_preserve_state_over_real_listener(), luau_anchored_registration_requires_capability_and_rejects_caught_invalid_geometry_before_save(), package(), sources(), luau_anchored_registration_rejects_unknown_sources_fields_and_out_of_range_bounds(), luau_anchored_then_storage_or_machine_same_block_rejects_caught_ownership_collision()

### Community 161 - "Decision"
Cohesion: 0.13
Nodes (6): Behavior, Decision, Event, EventKind, Registration, State

### Community 162 - "TransportStats"
Cohesion: 0.08
Nodes (10): CodecWorkers, decode_worker(), DECODE_WORKERS, DecodeRequest, encode_worker(), ENCODE_WORKERS, EncodedFrame, EncodeRequest (+2 more)

### Community 163 - "script_startup/creature.rs"
Cohesion: 0.13
Nodes (12): Flat, luau_creature_interaction_and_animation_negotiate_and_keep_private_state(), luau_creature_negotiates_model_ticks_and_restarts(), luau_creature_neighbour_policy_negotiates_and_reads_bounded_public_views(), luau_creature_options_reject_invalid_bounds_before_save_creation(), luau_creature_rejects_invalid_declaration_before_save_and_caught_route_failure(), luau_creature_rejects_invalid_lifecycle_before_movement_or_state_change(), luau_creature_spawns_another_declared_type_with_its_own_initial_state() (+4 more)

### Community 164 - "Growth foundation plan"
Cohesion: 0.10
Nodes (20): 1. Widen identities and introduce block states as one vertical format transition, 2. Deliver the missing parallel gameplay runtime, 3. Complete generic entity lifecycle and prove anchored behavior with gameplay, 4. Scale the real multiplayer path beyond the former 16-client ceiling, 5. Acceptance gate before calling this foundation complete, Baseline before this campaign (historical), Binding technical requirements, Decisions and invariants (+12 more)

### Community 165 - "world/generation/tests.rs"
Cohesion: 0.14
Nodes (9): AcrossSeam, authoritative_generation_edit_baselines_survive_cache_miss_and_restart(), Charged, files(), generation_failure_never_installs_air_or_replaces_recovery_snapshot(), generation_identity_mismatch_rejects_before_any_save_mutation(), open(), Pattern (+1 more)

### Community 166 - "Service"
Cohesion: 0.09
Nodes (8): handle(), Interest, invalidate(), MAX_CACHE, MAX_CLIENT_REQUESTS, MAX_PENDING, poll(), Service

### Community 167 - "common.rs"
Cohesion: 0.12
Nodes (6): action_result(), drop_stack(), JoinedSnapshot, Session, TEST_SAVE_SEQUENCE, TestSave

### Community 168 - "render/effects.rs"
Cohesion: 0.13
Nodes (11): Descriptor, prepare(), MAX_DESCRIPTOR_BYTES, MAX_PASSES, MAX_SHADER_BYTES, Pass, prepare(), prepare_inner() (+3 more)

### Community 169 - "Adapter"
Cohesion: 0.15
Nodes (4): Adapter, error(), register(), spawn_clear()

### Community 170 - "view.rs"
Cohesion: 0.17
Nodes (14): SlotFilter, draw(), footer(), GOLD, header(), inventory(), machine(), MUTED (+6 more)

### Community 171 - "drops/queries.rs"
Cohesion: 0.15
Nodes (19): DropLifetime, DropPickupContext, age_ms_now(), airborne_count(), capture_nearby(), collect_in_aabb(), distance_sq(), extractable() (+11 more)

### Community 172 - "client/audio/obstruction.rs"
Cohesion: 0.11
Nodes (18): block(), Job, MAX_CHUNKS, MAX_DISTANCE, MAX_RAY_CELLS, MAX_SOURCES, MIN_GAIN, obstruction() (+10 more)

### Community 173 - "world/tests.rs"
Cohesion: 0.10
Nodes (24): broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline(), chunk_cache_evicts_the_least_recently_used_resident(), composed_generation_is_ordered_and_does_not_change_builtin_baseline(), composed_generation_rejects_unknown_states_and_duplicate_keys(), corrupt_save_is_not_silently_discarded(), edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot(), edits_survive_restart_and_cache_eviction(), FixedGeneration (+16 more)

### Community 174 - "gameplay/decisions.rs"
Cohesion: 0.15
Nodes (10): BLOCK_REGISTER, BLOCK_SOURCE, ENTITY_REGISTER, ENTITY_SOURCE, entity_target_action_is_discovered_in_verified_session_catalog(), Fixture, luau_decisions_block_events_caught_error_rollback_and_restart(), luau_decisions_owned_entity_state_schedule_and_restart() (+2 more)

### Community 175 - "item_visuals.rs"
Cohesion: 0.12
Nodes (7): Cache, Entry, Key, MAX_ENTRIES, QUEUE, State, Visual

### Community 176 - "handles.rs"
Cohesion: 0.13
Nodes (14): entity(), entity_value(), EntityId, identity_methods(), intern(), profile(), profile_value(), ProfileId (+6 more)

### Community 177 - "Execution"
Cohesion: 0.11
Nodes (9): Engine, isolated(), MAX_RESIDENT, NEXT_RUNTIME, Reservation, RESIDENT, Retained, RUNTIMES (+1 more)

### Community 178 - "lifecycle-fixture/src/system.rs"
Cohesion: 0.12
Nodes (12): Clock, definition(), KEY, NeighborProbe, Pair, pair_definition(), Probe, WakeLoop (+4 more)

### Community 179 - "moving/load.rs"
Cohesion: 0.15
Nodes (7): ACTION, count(), moving_real_listener_capacity_measurements(), package(), rank(), REGISTER, request()

### Community 180 - "custom.rs"
Cohesion: 0.17
Nodes (12): compose(), Descriptor, Material, MaterialSource, MAX_DESCRIPTOR_BYTES, MAX_MATERIALS, MAX_SHADER_BYTES, prepare() (+4 more)

### Community 181 - "init"
Cohesion: 0.15
Nodes (3): DEFAULT_FILTER, filter(), init()

### Community 182 - "Scripting capabilities for mod developers"
Cohesion: 0.07
Nodes (32): module(), Actions, commands and gameplay decisions, Additional composable APIs, Authored materials, effects and Luau parameters, Authoring tools and runnable examples, Client startup and authored UI, Committed server observations, Durable owner systems (+24 more)

### Community 184 - "Stack"
Cohesion: 0.15
Nodes (6): ComponentPayload, HOTBAR_SLOTS, MAX_COMPONENT_BYTES, SLOTS, Stack, STACK_LIMIT

### Community 185 - "render/tests.rs"
Cohesion: 0.07
Nodes (18): ChunkMesh, emit_plant(), emit_quad(), GpuMesh, GpuSubmesh, mesh_chunk(), mesh_chunk_lit(), mesh_chunk_lit_with_catalog() (+10 more)

### Community 186 - "effects/registered.rs"
Cohesion: 0.05
Nodes (39): EffectConsumerBatch, EffectConsumerOutput, EffectConsumerScratch, EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen, EffectRegistryError, ErasedEffectKind (+31 more)

### Community 188 - "AnchoredBlockEntity"
Cohesion: 0.10
Nodes (3): AnchoredBlockEntity, Catalog, Catalog

### Community 189 - "articulated.py"
Cohesion: 0.05
Nodes (22): write(), Luau authoring in VS Code, Validate, Custom Luau machine processing, png(), convert(), geometry(), world() (+14 more)

### Community 190 - "voxel_view.rs"
Cohesion: 0.11
Nodes (13): air_chunk(), key(), MissingChunk, MovementError, player_collides(), resolve_player_movement(), resolve_player_movement_with_body(), set_block() (+5 more)

### Community 191 - "validate_spawn_volume"
Cohesion: 0.12
Nodes (4): prepare(), moving_launch_fences_complete_body_and_uses_final_terrain_overlay(), validate_spawn_volume(), WorldSnapshot<'_>

### Community 192 - "script_startup/gameplay.rs"
Cohesion: 0.15
Nodes (8): Fixture, luau_action_loopback_rollbacks_exact_transfer_receipts_and_restart(), luau_action_planner_errors_and_unavailable_retry_are_atomic(), luau_action_registration_and_persisted_source_identity_fail_closed(), Peer, PROFILE, REGISTER, SOURCE

### Community 193 - "client/bundle.rs"
Cohesion: 0.13
Nodes (15): CACHE, install(), invalid(), receive(), receive_progress(), session_references_released(), TEST_CACHE_LOCK, BundleIdentity (+7 more)

### Community 194 - "sealed_neighborhood"
Cohesion: 0.13
Nodes (10): key_offset(), incoming(), bounced_mode_reflects_surface_color_without_leaking_into_default(), distant_streamed_roof_blocks_and_reopens_a_deep_shaft(), emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides(), mapped_glowstone_definition_supplies_emission_to_light_builder(), opening_a_roof_shaft_relights_the_cave(), plants_and_leaves_transmit_daylight() (+2 more)

### Community 195 - "Clock"
Cohesion: 0.10
Nodes (6): Capture, Clock, DOMAIN, publish(), ReadStamp, state_key()

### Community 196 - "parallel/tests.rs"
Cohesion: 0.16
Nodes (17): spread_over_tcp(), batch(), bounded_queue_reports_saturation_without_accepting_a_partial_job(), cancellation_skips_queued_work_and_marks_running_results_cancelled(), completed_jobs(), dependency_waves_can_commit_twice_in_one_tick_and_later_wave_sees_prior_result(), job_errors_and_panics_reach_the_barrier_and_the_pool_keeps_running(), key() (+9 more)

### Community 197 - "Proposal: one coherent gameplay API"
Cohesion: 0.11
Nodes (19): 10. Custom models: required direction, deferred work, 1. A small, powerful set of concepts, 2. Atomic operations are a convenience, not a burden, 3. Specialized APIs are optional conveniences, 4. Built-in gameplay uses the same boundary, 5. Distinguish genuinely different execution contexts, 6. Developer experience is part of the implementation, 7. Server-delivered mod packages (+11 more)

### Community 198 - "Sender"
Cohesion: 0.10
Nodes (11): channel(), Sender, Shared, State, background_backlog_cannot_fill_edit_capacity_and_invalidation_removes_queued_work(), hot_edit_coalesces_and_promotes_without_losing_background_progress(), job(), shutdown_wakes_idle_workers() (+3 more)

### Community 199 - "render/material.rs"
Cohesion: 0.19
Nodes (15): blend_opposite_pixels(), emission_strengths(), face_uv(), item_material_layer(), item_material_layer_for(), material_layer(), material_layer_for(), material_mips() (+7 more)

### Community 200 - "fields_with_command"
Cohesion: 0.25
Nodes (10): block(), fields(), fields_with_command(), motion(), motion_contact(), moving_target(), captured_motion_contact_preserves_exact_readonly_target_and_revision(), every_public_removal_cause_preserves_its_exact_luau_context() (+2 more)

### Community 201 - "script/gameplay.rs"
Cohesion: 0.12
Nodes (4): Declaration, declarer(), registration(), ScriptHandler

### Community 202 - "Runtime"
Cohesion: 0.15
Nodes (4): count(), number(), own_key(), Runtime

### Community 203 - "thunder.rs"
Cohesion: 0.10
Nodes (17): add_bands(), BANDS, Build, build_voice(), Echo, ECHOES, length(), limit() (+9 more)

### Community 204 - "UiFrame<'_>"
Cohesion: 0.10
Nodes (9): draw(), choice(), color(), draw(), CharacterPanel, UiDebug, UiFrame<'_>, .BINDING_VIEW_PREFIX (+1 more)

### Community 205 - "players/lifecycle.rs"
Cohesion: 0.12
Nodes (13): PlayerOperation, PlayerOperationKind, committed(), drive(), enqueue(), Job, joined(), key() (+5 more)

### Community 206 - "Update"
Cohesion: 0.27
Nodes (8): defaults(), Definition, identifier(), Kind, MAX_PARAMETERS, State, Update, Value

### Community 207 - "Adapter"
Cohesion: 0.16
Nodes (3): Adapter, offset(), register()

### Community 208 - "VisualFire"
Cohesion: 0.09
Nodes (12): EffectBuffer, LIFE, MAX_EMBERS, FireRenderer, FireStyle, FLOATS, MAX_BYTES, MAX_FIRES (+4 more)

### Community 209 - "cold.rs"
Cohesion: 0.29
Nodes (11): ACTION, BEHAVIOR, body(), cold_resume_fixture(), dormant_fixture(), launch_fixture(), moving_real_listener_cold_terrain_and_dormancy_preserve_then_resume_saved_motion(), open_boxed() (+3 more)

### Community 210 - "machine_component_tests.rs"
Cohesion: 0.20
Nodes (9): exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers(), independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart(), load_neighbours(), payload(), public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports(), pulse(), SelectiveMachine, settle() (+1 more)

### Community 211 - ".compose_current_package_action_with_args"
Cohesion: 0.22
Nodes (8): ActionChoice, compose_named_command(), compose_observed_entity_action(), compose_package_action(), compose_package_action_with_args(), PackageActionInput, authored_entity_action_uses_observed_identity_and_exact_bounded_arguments(), named_shortcut_is_inert_without_matching_session_command()

### Community 213 - "sandbox.rs"
Cohesion: 0.19
Nodes (13): avatars(), Builder, camera(), factory(), neon(), prepare(), prepare_with_catalog(), Shot (+5 more)

### Community 214 - "SystemDescriptor"
Cohesion: 0.05
Nodes (31): FireDeliveryHandler, FireHandler, access(), AccessKind, BudgetKind, depends_on(), ExecutableSystem, IdentifierError (+23 more)

### Community 215 - "neighborhood.rs"
Cohesion: 0.16
Nodes (16): luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect(), luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes(), luau_neighborhood_radius_requires_exact_bounds_and_world_capability(), fixture(), GROW, id(), inbox(), load_neighborhoods() (+8 more)

### Community 216 - "install"
Cohesion: 0.23
Nodes (8): field(), install(), optional_vector(), parse_change(), parse_spawn(), SpawnRef, table(), vector()

### Community 217 - "CoordinatorContext"
Cohesion: 0.15
Nodes (16): join_named_client(), durable_actions(), ecology(), fire_delivery(), fire_source(), input_authorization(), interaction_commit(), player_movement() (+8 more)

### Community 218 - "invoke"
Cohesion: 0.18
Nodes (6): captured_entity(), checked(), entity_identity(), invoke(), push_wake(), read_block()

### Community 219 - "MovingEntity"
Cohesion: 0.10
Nodes (11): Body, CollisionMask, MAX_ACCELERATION, MAX_LIFETIME_TICKS, MAX_SOURCE_EXCLUSION_TICKS, MAX_SPEED, MovingEntity, Response (+3 more)

### Community 220 - "Mixer"
Cohesion: 0.08
Nodes (8): FRAMES, Limiter, QUEUE, UNIT, gains(), Mixer, Voice, Obstruction

### Community 221 - "wake.rs"
Cohesion: 0.15
Nodes (10): blocked(), canonical_wakes(), EntityWake, interact_producer(), INTERACT_PRODUCER_ID, MAX_WAKES_PER_PLAN, route_wakes(), TICK_PRODUCER_ID (+2 more)

### Community 222 - "Player"
Cohesion: 0.11
Nodes (6): Context<'_>, Player, capture(), publish_roster(), install(), present()

### Community 223 - "record.rs"
Cohesion: 0.13
Nodes (18): Cursor, Cursor<'a>, ExpiryReason, Impact, invalid(), length(), Motion, Pending (+10 more)

### Community 224 - "ScriptCreature"
Cohesion: 0.12
Nodes (4): invalid(), ScriptCreature, State, tick_helpers_keep_order_and_exact_intervals_above_float_precision()

### Community 225 - "Preparation"
Cohesion: 0.14
Nodes (3): Preparation, Ready, Renderer

### Community 226 - "click"
Cohesion: 0.16
Nodes (10): click(), label(), native_species_selector_audition_and_thunder_button_are_wired(), graphics_settings_do_not_offer_a_model_override(), label_center(), native_character_menu_keeps_apply_visible_and_blocks_repeat_while_pending(), native_graphics_adjusters_dispatch_their_own_controls(), native_graphics_lod_values_cover_off_horizons_and_quality_choices() (+2 more)

### Community 227 - "draw_image"
Cohesion: 0.21
Nodes (6): draw_image(), render_async(), render_lod_previews(), structure_tile(), terrain_meshes(), touches()

### Community 228 - "host-api/src/machine.rs"
Cohesion: 0.11
Nodes (17): Behavior, Context, DownwardFlow, FACES, Filter, Fuel, Machine, Plan (+9 more)

### Community 229 - "position_store.rs"
Cohesion: 0.18
Nodes (9): checksum(), invalid(), LEN, MAGIC, PositionStore, TEMP_SEQUENCE, validate_position(), validate_profile() (+1 more)

### Community 230 - "creature/services.rs"
Cohesion: 0.17
Nodes (9): coordinate(), guarded(), integer_cell(), invoke(), LifecycleRequest, parse_lifecycle(), parse_spawn(), SpawnRequest (+1 more)

### Community 231 - "native.rs"
Cohesion: 0.15
Nodes (5): fixture(), ImpactOnly, moving_native_generic_spawn_cannot_inject_a_valid_host_envelope(), moving_native_impact_only_reaction_resumes_physics_without_a_tick_callback(), PermissiveState

### Community 232 - "Context<'_>"
Cohesion: 0.26
Nodes (4): Context<'_>, Entity, EntityChange, EntitySpawn

### Community 233 - "Composition"
Cohesion: 0.14
Nodes (9): Catalog, Composition, field(), MAX_MEMBERS, MAX_PACKAGES, MAX_TAGS, Package, tag_kind() (+1 more)

### Community 234 - "inventory/container.rs"
Cohesion: 0.26
Nodes (5): decode(), encode(), invalid(), max_bytes(), independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data()

### Community 235 - "script_startup/appearance.rs"
Cohesion: 0.19
Nodes (6): appearance_bundle_composes_with_rules_animation_and_exact_world_identity(), CALL, invalid_or_caught_appearance_declarations_never_publish(), Peer, registered_appearance_selection_replicates_and_restarts_by_profile(), source()

### Community 236 - "render/pipeline.rs"
Cohesion: 0.17
Nodes (10): create_custom_voxel_pipeline(), create_sun_shadow_pipelines(), create_voxel_pipeline(), create_voxel_pipeline_source(), create_voxel_pipeline_with_catalog(), DETAIL_SHADER, PARALLAX_SHADER, RELIEF_SHADER (+2 more)

### Community 237 - "WeatherSnapshot"
Cohesion: 0.19
Nodes (8): extreme_weather_clock_does_not_overflow_lightning_time(), Lightning, mix(), regional_strikes_are_stable_world_positions_near_far_and_negative_players(), transitions_are_continuous_and_lightning_is_shared(), WeatherKind, WeatherSnapshot, WeatherValues

### Community 238 - "entity_sleep.rs"
Cohesion: 0.34
Nodes (18): queue_interaction_actions(), dispatch(), edit(), empty_action(), live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery(), new_sleepers_cannot_extend_the_current_recheck_pass(), position(), receive() (+10 more)

### Community 239 - "KilnPayload"
Cohesion: 0.05
Nodes (32): FUEL_SLOT_INDEX, fuel_ticks(), INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS, KILN_MAX_FUEL_TICKS, KILN_MAX_PAYLOAD_BYTES, KILN_MAX_RECIPES, KILN_TICK_INTERVAL (+24 more)

### Community 240 - "Execution foundation: next implementation slices"
Cohesion: 0.12
Nodes (16): 1. Scheduling and progress under capacity pressure, 2. Reliable wake and sleep semantics, 3. Separate conflict revisions from publication ordering, 4. Consolidate commit orchestration and make barriers explicit, 5. Off-thread publication and bounded checkpoint work, Execution approach, Execution foundation: next implementation slices, First: review the current worker slice — done (+8 more)

### Community 241 - "server/appearance.rs"
Cohesion: 0.20
Nodes (7): checksum(), MAX_LEN, replace(), select(), select_character(), SEQUENCE, Store

### Community 242 - "script_startup.rs"
Cohesion: 0.19
Nodes (7): CONTENT, Fixture, luau_failed_restart_leaves_existing_save_unchanged_and_can_retry(), luau_startup_item_reaches_listener_inventory_and_restart(), luau_startup_rejections_publish_nothing_and_never_open_world(), luau_startup_validates_contracts_and_runs_imports_at_the_item_bound(), TOKEN

### Community 243 - "process_movement_batch"
Cohesion: 0.31
Nodes (18): MovementCommand, process_movement_batch(), air_chunk(), close(), command(), command_order_is_stable_and_affects_the_authoritative_position(), crouch_geometry_budget_and_unknown_standing_are_authoritative(), invalid_and_excessive_deltas_consume_sequence_without_changing_position() (+10 more)

### Community 244 - "contact_shadow.rs"
Cohesion: 0.10
Nodes (22): append(), MAX_CELLS, MAX_CHARACTERS, MAX_DISTANCE, MAX_HEIGHT, MAX_PATCHES, Patch, patches() (+14 more)

### Community 245 - "Contributor"
Cohesion: 0.13
Nodes (8): Contributor, coordinate(), Declaration, declarer(), invoke(), registration(), runtime(), ScriptContributor

### Community 246 - "request_chunk"
Cohesion: 0.08
Nodes (22): block_intersects_player(), plan(), denied(), Invocation, plan(), read_target(), sight(), verify_reach() (+14 more)

### Community 247 - "authored.rs"
Cohesion: 0.12
Nodes (7): ATLAS_SIZE, Document, INVALID, json(), MAX_TEXT, Resources, Widget

### Community 248 - "src/composition.rs"
Cohesion: 0.13
Nodes (14): ACTIONS, ANCHORED_ENTITIES, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, ITEM_ICONS, MACHINES (+6 more)

### Community 249 - "Slots"
Cohesion: 0.19
Nodes (3): Slots, StoragePayload, StoragePayload<N>

### Community 251 - "client/lifecycle/tests.rs"
Cohesion: 0.26
Nodes (9): exercise_join_lifecycle(), exercise_player_services(), join(), player_notices_present_only_current_session_and_kicks_retire_it(), player_roster_accepts_newer_snapshots_and_is_cleared_when_session_retires(), public_player_snapshots_validate_session_and_services_then_clear_on_retirement(), read(), retired() (+1 more)

### Community 252 - "parse"
Cohesion: 0.13
Nodes (14): declarer(), field(), optional_integer(), optional_text(), owned(), parse(), state_key(), boolean() (+6 more)

### Community 253 - "Runtime"
Cohesion: 0.08
Nodes (14): check(), LOG_RANGE, MAX_SKY_SCAN, read(), Rule, supported(), CELLS_PER_CHUNK, CHUNKS_PER_TICK (+6 more)

### Community 254 - "package/tests.rs"
Cohesion: 0.25
Nodes (11): cycles_depth_and_shared_execution_budget_are_bounded(), dependency_versions_and_manifest_declarations_are_strict(), discovery_bounds_directory_count_source_bytes_and_total_bytes(), discovery_rejects_symlinks_at_every_path_level_and_special_files(), failed_modules_are_not_reinitialized_when_caught(), Fixture, import_failures_name_package_version_and_module_and_do_not_poison_worker(), imported_source_limits_and_exported_function_errors_keep_source_identity() (+3 more)

### Community 255 - "systems/world.rs"
Cohesion: 0.14
Nodes (6): capture(), capture_entities(), EditInputs, plan_changes(), plan_edits(), within_radius()

### Community 256 - "bake_occlusion.py"
Cohesion: 0.15
Nodes (13): bake_visibility(), build_sidecar(), cross(), dot(), hemisphere_directions(), main(), normalize(), ray_triangle() (+5 more)

### Community 257 - "paint_icon"
Cohesion: 0.11
Nodes (7): draw(), draw(), color_from_swatch(), paint_icon(), show(), SlotStyle, item_visuals_egui_slots_paint_the_worker_selected_stack_icon()

### Community 258 - "State"
Cohesion: 0.13
Nodes (7): column_cover(), COVER_PERIOD, COVER_SIDE, flash_at(), scan_ceiling(), State, super::ClientApp

### Community 259 - "LodTile"
Cohesion: 0.11
Nodes (13): Column, Interval, LodTile, MAX_LEVEL, MAX_SPANS_PER_COLUMN, MAX_TILE_BYTES, MAX_TILE_COVERAGE, MAX_TILE_SPANS (+5 more)

### Community 260 - "EntityTransferPolicy"
Cohesion: 0.07
Nodes (11): Adapter, Adapter, Adapter, AutomationStack, EntityItemTransfer, EntityTransferPolicy, movable_count(), move_up_to() (+3 more)

### Community 262 - "Gpu"
Cohesion: 0.13
Nodes (3): Data, Gpu, MaterialData

### Community 264 - "resolve_nodes"
Cohesion: 0.15
Nodes (13): identifier(), owned(), bounded_nodes(), control(), dynamic_control_forest_validates_choices_ownership_and_container_parents(), Kind, Presentation, raw() (+5 more)

### Community 265 - "owner_durable/tests.rs"
Cohesion: 0.36
Nodes (15): byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation(), capacity_defers_while_corruption_stops(), chunk(), commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing(), committed_waves_mark_active_and_update_the_due_index(), counter_store(), dropped_prepared_wave_changes_nothing(), fed_deadlines_are_not_duplicated_and_replay_clears_stale_readiness() (+7 more)

### Community 266 - "system/intents.rs"
Cohesion: 0.18
Nodes (12): luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id(), luau_intent_send_requires_opt_in_and_absence_requires_bootstrap(), CHAIN, id(), inbox(), luau_intent_absent_destinations_run_on_real_listener_and_recover_once(), luau_intent_caught_invalid_and_overbudget_sends_poison_all_output(), luau_intent_declarations_and_session_identity_are_owned_and_bounded() (+4 more)

### Community 267 - "Committed"
Cohesion: 0.17
Nodes (7): Committed, UseObserver, luau_committed_observer_timeout_cannot_block_receipt_or_later_observer_over_listener(), Witness, entity(), present(), seed()

### Community 268 - "client/world.rs"
Cohesion: 0.21
Nodes (7): Cursor, MAX_PENDING_GROUPS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot, WorldProbe, WorldUpdate

### Community 269 - "registry/tests.rs"
Cohesion: 0.20
Nodes (19): builtin_phase_plan(), register_builtin_systems(), declarations_match_the_current_execution_shape(), descriptor(), deterministic_plan(), disjoint_and_read_only_accesses_can_share_a_phase(), duplicate_ids_and_invalid_namespaced_ids_are_rejected(), freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() (+11 more)

### Community 270 - "Hit"
Cohesion: 0.10
Nodes (26): no_hit(), no_interact(), PLAYER_ENTITY_TYPE, project_avatar(), INSERT_FUEL, INSERT_INPUT, interact_verb(), interaction() (+18 more)

### Community 271 - "Imports"
Cohesion: 0.11
Nodes (4): Imports, MAX_IMPORT_DEPTH, ModuleState, Compiled

### Community 272 - "OwnerApplyReceipt"
Cohesion: 0.22
Nodes (3): OwnerApplyReceipt, OwnerApplyTask, World

### Community 273 - "src/lod/tests.rs"
Cohesion: 0.12
Nodes (9): extract(), merge(), reduce_parent(), chunk(), coarse_extraction_requires_every_horizontal_sample(), composed_contributor_bridge_survives_bounded_render_extraction(), empty_is_known_missing_is_not_and_seams_join(), parent_maps_child_quadrants_and_rejects_wrong_children() (+1 more)

### Community 274 - "Chunk"
Cohesion: 0.10
Nodes (10): AudioState, Capture, CAPTURE_RADIUS, INTERVAL, position_matches(), Ready, RESULT_TIMEOUT, State (+2 more)

### Community 275 - "entities/motion/tests.rs"
Cohesion: 0.36
Nodes (22): integrate(), acceleration_and_capture_are_deterministic_across_chunk_seams(), body(), budgets_and_invalid_inputs_reject_instead_of_truncating(), corner_bounces_resolve_multiple_contacts_and_capacity_stops_safely(), dynamic_frame_bounce_obeys_declared_speed_cap(), earlier_wall_contact_prevents_false_world_boundary_removal(), embedded_contact_has_stable_unit_normal_without_pushout() (+14 more)

### Community 276 - "Observations"
Cohesion: 0.16
Nodes (11): ActionView, BlockView, ComponentView, InventoryView, key(), Observations, revision(), SlotView (+3 more)

### Community 277 - "actions/workstation.rs"
Cohesion: 0.20
Nodes (6): audible(), preview(), place(), Placement, Removal, remove()

### Community 278 - "entity_checkpoint/tests.rs"
Cohesion: 0.18
Nodes (13): admitted_event_permit_cannot_be_dropped_silently(), checkpoint_io_failure_closes_admission_and_reports_error(), delayed_payload_receipt_keeps_newer_checkpoint_only_motion(), fixture(), interrupted_stream_does_not_publish_and_worker_failure_releases_credit(), malformed_ordered_event_fails_closed_without_checkpoint_publication(), multi_turn_checkpoint_holds_generation_fence_without_a_live_capture(), NEXT_TEST_DIR (+5 more)

### Community 279 - "System"
Cohesion: 0.11
Nodes (15): DropSpawn, EntityChange, EntitySpawn, IntentId, IntentOutbox, IntentRequest, MAX_INTENT_PAYLOAD_BYTES, MAX_INTENTS_PER_JOB (+7 more)

### Community 280 - "Scripting Gap Closure Plan"
Cohesion: 0.15
Nodes (13): Coverage and implementation order, Design requirements, Milestone 1 Authoring foundations, Milestone 2 Gameplay control and native parity, Milestone 3 World jobs and structured observations, Milestone 4 Dynamic UI, Milestone 5 Audio, Milestone 6 Models geometry and physics (+5 more)

### Community 282 - "server/movement.rs"
Cohesion: 0.14
Nodes (12): AckKind, credit_per_tick(), CREDIT_SCALE, FLOAT_ROUNDING_ALLOWANCE_PER_TICK, MAX_COMMANDS_PER_TICK, max_credit(), movement_cost(), MovementAck (+4 more)

### Community 283 - "server/drops.rs"
Cohesion: 0.13
Nodes (8): DROP_RADIUS, GRAVITY, invalid(), is_drop_delta(), LIFETIME, TERMINAL_SPEED, VIEW_RANGE, VIEW_RANGE_SQ

### Community 286 - "build_stream"
Cohesion: 0.19
Nodes (5): build_stream(), open_device(), write_output(), fill(), Frame

### Community 287 - "protocol/drops.rs"
Cohesion: 0.20
Nodes (9): DroppedItem, FIXED_ITEM_BYTES, items_wire_len(), MAX_ITEMS, pickup_pages(), prefix_count(), read_items(), snapshot_count() (+1 more)

### Community 288 - "EntityCodecError"
Cohesion: 0.11
Nodes (9): BinCodec, BinPayload, CounterCodec, MateCodec, WideCodec, ByteCodec, Adapter, EntityCodecError (+1 more)

### Community 290 - "script_startup/gameplay/player_inventory.rs"
Cohesion: 0.21
Nodes (12): concurrent_profile_inventory_transfers_retry_without_lost_items(), readonly_inventory_reservations_fence_writers_and_detect_stale_revisions(), corrupt_offline_inventory_rejects_caught_access_without_grant_or_server_failure(), cross_profile_inventory_requires_package_authority_even_for_reads(), fixture(), joined_cross_profile_inventory_and_state_commit_together_once(), offline_profile_tick_loads_asynchronously_and_commits_both_inventories(), oversized_profile_inventory_transaction_is_denied_and_next_wal_action_succeeds() (+4 more)

### Community 292 - "kiln/codec.rs"
Cohesion: 0.28
Nodes (5): decode_stack(), encode_payload(), encode_stack(), KILN_PAYLOAD_VERSION, KilnPayloadCodec

### Community 293 - "ItemIcon"
Cohesion: 0.10
Nodes (7): ItemIcon, definitions(), Catalog, decode(), fields(), invalid(), icon_decoder_rejects_sparse_unknown_oversized_and_nonfinite_art()

### Community 295 - "character_asset/gameplay/tests.rs"
Cohesion: 0.15
Nodes (3): final_head_look_clamps_animation_and_input_to_the_hair_envelope(), planted_stance_and_clear_swing_feet_follow_actual_mesh_through_all_gait_blends(), relative()

### Community 296 - "actions/command.rs"
Cohesion: 0.08
Nodes (18): Command, command_alias(), command_text(), CommandArgument, CommandValue, FiniteNumber, MAX_COMMAND_ALIASES, MAX_COMMAND_ARGUMENTS (+10 more)

### Community 297 - "Atmosphere"
Cohesion: 0.11
Nodes (8): Atmosphere, shader(), smooth(), surface_shader(), create_sky_pipeline(), sky_camera_data(), SKY_SHADER, sky_basis_tracks_camera_turns_in_world_space()

### Community 299 - "time"
Cohesion: 0.06
Nodes (25): accepted_work_budget_has_explicit_nonblocking_overflow(), pre_edit_worker_result_is_rejected_after_uncheckpointed_edit(), receive_before(), requests_are_deduplicated_and_negative_chunks_load_asynchronously(), test_dir(), TEST_DIR_COUNTER, worker_load_uses_uncheckpointed_authoritative_snapshot_after_eviction(), NEXT (+17 more)

### Community 300 - "client/startup.rs"
Cohesion: 0.18
Nodes (12): ascii(), display(), execute(), execute_event(), execute_retained(), identifier(), identity(), item_visual_declarer() (+4 more)

### Community 301 - "Inputs"
Cohesion: 0.16
Nodes (3): expand(), Expansion, Inputs

### Community 302 - "gameplay/admin.rs"
Cohesion: 0.20
Nodes (5): Admin, GIVE, SPAWN, TIME, WEATHER

### Community 303 - "script/capacity.rs"
Cohesion: 0.10
Nodes (21): BLOCKS_PER_PACKAGE, CLIENT_PREPARATION_WALL_TIME, GENERATION_SCRIPT_WALL_TIME, GENERATORS_PER_PACKAGE, INSTALLATION_WALL_TIME, ITEMS_PER_PACKAGE, MAX_ASSET_BYTES, MAX_ASSETS (+13 more)

### Community 304 - "EntityDependencies"
Cohesion: 0.13
Nodes (3): EntityDependencies, EntityStore, PreparedEntityTransaction

### Community 305 - "Downloads"
Cohesion: 0.10
Nodes (4): complete_package_handshake(), Downloads, receive_package(), state_with_package()

### Community 306 - "prepare"
Cohesion: 0.28
Nodes (3): accepts(), capture(), prepare()

### Community 307 - "lod/worker.rs"
Cohesion: 0.20
Nodes (7): build(), cache_checksum(), Completion, Job, load_cached(), run(), trim_cache()

### Community 308 - "prepare"
Cohesion: 0.46
Nodes (3): builtin(), builtin_cached(), prepare()

### Community 309 - "ModelRenderer"
Cohesion: 0.12
Nodes (4): Material, MaterialUniform, Mesh, ModelRenderer

### Community 310 - ".new"
Cohesion: 0.38
Nodes (6): invalid_data(), read_world_metadata(), resolve_content_map_with(), verify_content_map_with(), verify_new_world_directory(), write_content_map()

### Community 312 - "_"
Cohesion: 0.15
Nodes (12): _, BYTES_PER_ROW, FORMAT, HEIGHT, MAX_MEASURED_FRAMES, run(), run_character_benchmark(), scene() (+4 more)

### Community 314 - "src/client/tests.rs"
Cohesion: 0.07
Nodes (21): audio_controls_survive_character_settings_reconciliation(), block_edit_uses_selected_hotbar_block_and_hit_face(), confirmed_fire_visuals_expire_and_are_capped_and_distance_culled(), graphics_controls_apply_save_and_preserve_values_while_disabled(), lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently(), latest_edit_mesh_survives_a_superseded_kiln_relight_backlog(), mapped_server_item_and_replaceable_state_drive_placement_preview(), moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness() (+13 more)

### Community 315 - "view"
Cohesion: 0.12
Nodes (17): Mossbun, BODY, gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling(), ground_motion_respects_walls_cliffs_seams_and_embedded_edits(), view(), BODY, register(), a_new_obstacle_invalidates_the_current_waypoint_before_movement() (+9 more)

### Community 317 - "io"
Cohesion: 0.08
Nodes (15): BUDGET, MAX_TRANSIENT_BYTES, Reservation, reserve(), TURN_ENTRIES, WRITE_BYTES, MAX_FRONTIER_CELLS, Journal (+7 more)

### Community 318 - "Catalog"
Cohesion: 0.04
Nodes (55): ACTIVE, block_flags(), BlockDef, BlockTextures, BUILTIN_EMISSION, BUILTIN_FLAGS, BUILTIN_REFLECTANCE, Catalog (+47 more)

### Community 319 - "PlayerState"
Cohesion: 0.12
Nodes (8): Lane, Mailbox, Runner, Snapshot, PlayerState, read(), valid_key(), write()

### Community 320 - "ecology/tests.rs"
Cohesion: 0.15
Nodes (15): CELL, check(), connected_leaves_find_all_log_orientations_within_six_steps(), conversions_commit_after_receipt_and_recover_without_harvest_drops(), dirt_needs_daylight_and_a_clear_column_without_adjacent_grass(), edit(), grass_dies_under_solid_cover_but_plants_leave_it_alive(), missing_seam_never_counts_as_absent_log_support() (+7 more)

### Community 321 - "Mesh"
Cohesion: 0.17
Nodes (6): FaceColors, find_column(), Mesh, quad(), side_light(), subtract()

### Community 323 - "SignalPost"
Cohesion: 0.22
Nodes (3): KEY, SignalPost, State

### Community 324 - "Budget"
Cohesion: 0.16
Nodes (9): Budget, decode(), MAGIC, profile(), wrap(), cached_shared_artifacts_and_rejected_growth_release_exact_admission(), compact_artifacts_cannot_hide_metadata_from_process_admission(), metadata_admission_preserves_typed_pressure_with_declaration_context() (+1 more)

### Community 326 - "scheduling_tests.rs"
Cohesion: 0.23
Nodes (9): due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility(), increment(), invalid_handler_deadline_rejects_without_losing_due_work(), mixed_active_and_recurring_due_owners_progress_with_and_without_wakes(), multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation(), run(), scheduled_startup(), wake_advances_deadline_work_and_handler_can_return_to_active() (+1 more)

### Community 328 - "queries/tests.rs"
Cohesion: 0.31
Nodes (8): airborne_count_tracks_schedule_not_records(), component_drop_projection_keeps_nearest_complete_stacks_within_frame_budget(), expired_drop_is_visible_but_never_pickable(), immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation(), pickup_delay_gates_candidates_but_not_visibility(), single_drop_collection_rechecks_range_delay_and_expiry(), spawn_direct(), test_store()

### Community 330 - ".accept"
Cohesion: 0.13
Nodes (6): chunk_in_view(), ClientApp, lighting_depends_on(), mesh_priority(), ClientApp, ClientApp

### Community 331 - "2. Player and lifecycle hooks"
Cohesion: 0.05
Nodes (42): 1. Basic runtime tools — closed, 2. Player and lifecycle hooks, 3. Dynamic UI and input — closed within agreed scope, 4. General persistent block entities — closed, 5. Flexible entities, motion and presentation, 6. Development iteration and save continuity, 7. Content-pack scale and composition limits, Acceptance and examples (+34 more)

### Community 332 - "EntityError"
Cohesion: 0.03
Nodes (54): BinExchange, checked_body(), crc32(), Decoder, Decoder<'a>, Encoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION (+46 more)

### Community 333 - "public_systems/motion/tests.rs"
Cohesion: 0.17
Nodes (5): Bytes, catalog(), MotionOwner, owner_moving_spawn_wraps_record_and_rejects_missing_authority_or_capture(), spawn()

### Community 334 - "script_startup/generation.rs"
Cohesion: 0.26
Nodes (9): Fixture, GENERATION, luau_generation_baseline_edits_and_identity_survive_restart(), luau_generation_rejects_bad_registration_and_caught_output_errors(), luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads(), luau_generation_streams_from_loader_after_restart(), MARKER, REGISTER (+1 more)

### Community 335 - "Storage"
Cohesion: 0.24
Nodes (3): checksum(), SavedEdits, Storage

### Community 337 - "streaming/entities.rs"
Cohesion: 0.21
Nodes (5): MAX_PUBLIC_ENTITIES_PER_CHUNK, MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK, project(), snapshot_messages(), SnapshotError

### Community 338 - "script_startup/bundle.rs"
Cohesion: 0.24
Nodes (15): bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join(), bundle_restart_exact_cache_and_changed_source_require_new_bytes(), client_verification_rejects_relay_tamper_truncation_and_reordering(), closed(), fixture(), fragmented(), offer(), package_effect_is_prepared_by_real_client_join_before_welcome() (+7 more)

### Community 339 - "src/actions/tests.rs"
Cohesion: 0.18
Nodes (8): action(), command_facets_are_empty_gameplay_only_and_fingerprint_permissions(), composed_controls_resolve_forward_references_without_changing_target_context(), composition_bounds_and_fingerprint_cover_every_control(), declared_aliases_are_unique_and_do_not_create_authoritative_action_keys(), discovery_is_bounded_ordered_and_rejects_conflicting_ownership(), empty_console_commands_do_not_consume_generic_action_capacity(), ordered_command_schema_has_canonical_bounded_arguments_and_identity()

### Community 341 - "rig.rs"
Cohesion: 0.14
Nodes (13): apply_look(), CHEST, clamp_look(), HEAD, LEFT_ARM, LEFT_LEG, NAMES, NECK (+5 more)

### Community 342 - "tick/tests.rs"
Cohesion: 0.31
Nodes (10): drop_snapshot(), dropped_column_lands_at_rest_and_suspends(), falling_drop_integrates_exactly_one_fixed_step(), missing_terrain_defers_fail_closed_without_guessing(), neighbourhood_view(), neighbours(), NEXT_TEST_DIR, planner_rejects_anchored_locations_and_foreign_payloads() (+2 more)

### Community 343 - "player_services/tests.rs"
Cohesion: 0.23
Nodes (6): client_player_callback_rejection_is_atomic_and_other_packages_keep_running(), client_player_callbacks_compose_filter_public_state_and_disconnect_without_queue_room(), failed_client_import_initialization_is_cached_for_the_realm(), Fixture, retained_player_modules_keep_imports_and_coroutines_but_revoke_old_hosts(), states()

### Community 344 - "Bloxgloom interface plan"
Cohesion: 0.25
Nodes (8): Baseline when this plan was written, Bloxgloom interface plan, Delivery order, Goal, Interaction contract, Performance and correctness, UI and game-state design, Validation record

### Community 346 - "client/admin.rs"
Cohesion: 0.28
Nodes (6): BINDING_ROWS_PER_PAGE, binding_targets(), Command, parse(), parse_with_players(), registered()

### Community 347 - "nearest_unsent"
Cohesion: 0.23
Nodes (5): nearest_unsent(), boundaries_skip_unrepresentable_chunk_keys(), distance(), sent_keys_are_skipped_without_expanding_the_budget(), visits_every_interest_key_once_in_distance_order()

### Community 348 - "receive_content_manifest"
Cohesion: 0.07
Nodes (26): Blocks, inventories, and processing, external_anchored_initialization_use_refund_and_restart_over_real_listener(), external_owner_world_read_survives_real_listener_join_and_restart(), external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp(), external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener(), registered_item_components_transfer_and_recover_over_real_listener(), send(), storage_roundtrip() (+18 more)

### Community 349 - "prepare"
Cohesion: 0.18
Nodes (6): decode_motion_id(), invalid_data(), invalid_entity(), prepare(), PreparedEntityRecovery, validate_checkpointed_motion()

### Community 350 - "moving/lifecycle.rs"
Cohesion: 0.22
Nodes (10): ACTION, BEHAVIOR, install(), launch(), moving_failed_expiry_keeps_profile_inventory_and_drop_rewards_atomic(), moving_real_listener_admin_cancel_rejects_nonadmin_and_clears_stuck_reaction(), moving_real_listener_dynamic_targets_source_exclusion_and_exact_receipt_replay(), moving_real_listener_expiry_effect_is_once_and_replay_preserves_historical_id() (+2 more)

### Community 351 - "output.rs"
Cohesion: 0.17
Nodes (6): COMMAND_CAPACITY, OUTPUT_BATCH, OutputStats, RING_FRAMES, SHUTDOWN_WAIT, SOURCE_FRAMES

### Community 352 - "ContentManifest"
Cohesion: 0.20
Nodes (8): checksum(), ContentEntry, ContentManifest, invalid(), MAGIC, MAX_ENTRIES, MAX_MANIFEST_BYTES, VERSION

### Community 354 - "integer"
Cohesion: 0.04
Nodes (28): cell_at(), checked(), entity_id(), invalid(), invoke_fields(), position_at(), bytes(), install() (+20 more)

### Community 355 - "Bloxgloom"
Cohesion: 0.22
Nodes (9): Bloxgloom, Client execution and remaining extension work, Current execution architecture, Development and previews, HDR presentation, Publication and checkpoint boundaries, Run locally, Server threads and workers (+1 more)

### Community 357 - "drop_merge.rs"
Cohesion: 0.24
Nodes (4): DropMergeCandidate, DropMergeContext, DropStackFill, filling_and_splitting_conserve_items_at_the_stack_cap()

### Community 358 - "colliders.rs"
Cohesion: 0.21
Nodes (8): capture(), collider(), History, intersects(), MAX_HISTORY_CREATURES, Pose, push(), sample()

### Community 359 - "net/tests.rs"
Cohesion: 0.12
Nodes (14): client_commands_are_rejected_until_content_ready_matches(), complete_content_handshake(), join_cleanup_enqueues_one_leave_with_the_next_sequence(), local_server_shutdown_restores_authoritative_position_on_next_start(), committed_edit_refreshes_two_distant_clients_without_waiting_for_checkpoint(), distant_tile_streams_with_session_and_keeps_gameplay_ping_responsive(), registered_contributor_summary_negotiates_bounded_fallback_and_preserves_marker(), nonblocking_listener_streams_and_recovers_a_wal_acked_edit_inner() (+6 more)

### Community 360 - "server/checkpoint/tests.rs"
Cohesion: 0.27
Nodes (14): capacity_counts_running_jobs_and_unconsumed_receipts(), closure_panic_becomes_an_error_receipt_and_worker_keeps_running(), completed_but_unconsumed_receipt_still_occupies_capacity(), drop_drains_accepted_work_without_blocking_on_full_receipt_channel(), failed_write_is_returned_with_its_key_and_revision(), independent_checkpoint_keys_progress_while_another_shard_is_blocked(), key(), key_on_shard() (+6 more)

### Community 361 - "Distant terrain LOD implementation plan"
Cohesion: 0.11
Nodes (17): Distant terrain, Implementation, Measurements, 2026-10-01, Reproduce verification, Resource limits and caching, Data model and module boundaries, Distant Horizons references, Distant terrain LOD implementation plan (+9 more)

### Community 363 - "render/lod/tests.rs"
Cohesion: 0.20
Nodes (13): desired_tiles(), bridge_keeps_both_gap_faces_and_sides(), coarse_boundary_splits_only_where_fine_spans_differ(), column(), coordinate_and_vertical_extremes_mesh_without_wrapping(), detailed_ring_extends_past_near_chunks_with_complete_sibling_families(), fixture(), flat_roof_caps_merge_without_filling_openings() (+5 more)

### Community 364 - "EventRealm"
Cohesion: 0.16
Nodes (3): EventRealm, Export, load()

### Community 365 - "Fixture"
Cohesion: 0.18
Nodes (4): Fixture, gpu_terrain_shadow_edits_cutouts_and_sky_glow_invariance(), SIZE, verify_custom_alpha()

### Community 366 - "widgets.rs"
Cohesion: 0.27
Nodes (6): ControlValue, display_text(), nodes(), record(), sequence(), value()

### Community 367 - "durable/checkpoint.rs"
Cohesion: 0.24
Nodes (9): checkpoint_keys_turn(), decode_chunk_checkpoint_key(), decode_inventory_checkpoint_key(), fire_batch_key(), is_fire_checkpoint_key(), process_checkpoint_receipts(), submit_dirty_checkpoints(), submit_fire_checkpoint_batch() (+1 more)

### Community 368 - ".first_solid_top"
Cohesion: 0.20
Nodes (4): FallingContext, FallingPlan, FallingWorld, Column

### Community 369 - "script_startup/machine.rs"
Cohesion: 0.11
Nodes (21): luau_component_machine_processes_exact_stack_over_listener_and_recovers(), luau_dynamic_transformation_computes_components_over_listener_and_recovers(), luau_machine_component_options_reject_invalid_constants_before_save(), luau_machine_component_recipe_negotiates_exact_predicate_and_preservation(), luau_machine_present_input_exact_output_and_component_fuel_roundtrip(), luau_transformations_capture_owned_components_and_replay_deterministically(), process_over_listener(), source() (+13 more)

### Community 370 - "journal/tests.rs"
Cohesion: 0.11
Nodes (18): append_direct(), all_incomplete_append_prefixes_recover_to_the_last_complete_record(), complete_corrupt_record_and_invalid_header_are_rejected(), exact_legacy_header_prefixes_are_repaired_and_nonprefixes_fail_closed(), replay_deduplicates_identical_ids_and_rejects_conflicting_reuse(), writer_acknowledges_only_a_synced_transaction_and_reopens_it(), incomplete_or_rejected_records_cannot_advance_the_recovered_clock(), shared_clock_survives_tail_recovery_rotation_and_lower_tick_records() (+10 more)

### Community 371 - "scene"
Cohesion: 0.31
Nodes (8): aligned_and_offset_doorways_are_audible_and_closing_them_muffles(), coincident_unknown_endpoints_do_not_invent_clear_transmission(), negative_chunk_seam_and_only_embedded_endpoint_cell_are_respected(), scene(), terrain_profiles_muffle_fixture_motor_in_production_mixer(), thick_walls_reduce_transmission_and_unknown_space_never_invents_openings(), value(), walls_attenuate_high_frequencies_while_leaves_remain_porous()

### Community 372 - "coder.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 373 - "ensure"
Cohesion: 0.20
Nodes (14): Appearance, Color, control_name(), Controls, Layer, Look, Model, resolve() (+6 more)

### Community 374 - "coder-fast.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 377 - "PlayerSummary"
Cohesion: 0.23
Nodes (7): complete(), normalize(), token(), PlayerSummary, read(), validate_notice(), write()

### Community 378 - "declarer"
Cohesion: 0.19
Nodes (5): bytes(), cell(), declarer(), field(), table()

### Community 379 - ".draw_node"
Cohesion: 0.24
Nodes (4): Intent, rgba(), Session, trim_bytes()

### Community 380 - "install_sandbox_materials"
Cohesion: 0.18
Nodes (6): install_sandbox_materials(), block(), chunks(), emissive_fixture_registration_is_valid_and_isolated_from_builtins(), fixture_replaces_terrain_and_character_pose_does_not_move_between_themes(), themes_preserve_common_swatches_tree_and_camera()

### Community 381 - "decode"
Cohesion: 0.33
Nodes (3): bytes(), decode(), delay()

### Community 382 - "sample"
Cohesion: 0.23
Nodes (4): material(), sample(), declared_material_and_habitat_survive_states_and_only_sample_exposed_faces(), sample_tracks_canopy_ground_edits_and_unknown_chunks()

### Community 383 - "GpuPass"
Cohesion: 0.14
Nodes (3): Data, GpuPass, texture_entry()

### Community 384 - "material/companions.rs"
Cohesion: 0.18
Nodes (9): Maps, mips(), prepare(), builtin_companions_are_registered_without_changing_original_layers(), companion_keys_resolve_independently_of_registration_order_and_missing_maps_fall_back(), data_mips_are_linear_and_do_not_weight_normals_by_height_alpha(), register(), tile() (+1 more)

### Community 385 - "OwnerData"
Cohesion: 0.04
Nodes (19): OwnerCodec, OwnerData, Codec, config(), prepare(), prepare_writes(), OwnerCodecError, OwnerValueCodec (+11 more)

### Community 386 - "src/storage/tests.rs"
Cohesion: 0.33
Nodes (9): active_world_lock_excludes_a_second_writer_and_releases_on_drop(), content_map_preserves_wide_assignments_and_rejects_reassignment(), hidden_partial_conversion_stage_cannot_be_opened_even_after_marker_removal(), incomplete_conversion_cannot_be_opened_as_a_world(), old_world_is_rejected_without_creating_a_lock_or_rewriting_data(), temporary_root(), wide_sparse_edits_round_trip_and_reject_reordering(), with_extra_block() (+1 more)

### Community 389 - "install"
Cohesion: 0.14
Nodes (4): Capabilities, install(), Output, SpawnRef

### Community 390 - "content"
Cohesion: 0.06
Nodes (19): assign(), register(), kiln_block_states(), kiln_footprint(), kiln_payload(), kiln_state(), KilnHalf, register_entity_type() (+11 more)

### Community 392 - "resolve_player_movement"
Cohesion: 0.46
Nodes (4): MAX_MOVEMENT_STEPS_PER_AXIS, player_collides(), resolve_player_movement(), ResolveError

### Community 393 - "script_startup/moving.rs"
Cohesion: 0.17
Nodes (9): ACTION, BEHAVIOR, moving_failed_impact_remains_durable_and_does_not_grant_partial_reward(), moving_loopback_debits_once_sweeps_reacts_and_recovers_consumed_impact(), package(), prepare(), PROFILE, records() (+1 more)

### Community 396 - "world_to_chunk"
Cohesion: 0.13
Nodes (48): coordinator_admits_two_independent_atomic_pickups_before_applying_either(), external_storage_rejects_blocked_footprint_and_stale_placement_without_debit(), registered_slot_permissions_are_enforced_by_server_even_for_forged_requests(), anchored_break_uses_registered_removal_without_duplicating_refunds(), automatic_pickup_rejects_uncredited_take_before_wal_admission(), committed_observers_see_public_results_only_after_receipt_without_replay(), generic_entity_spawn_update_remove_composes_with_breaks_and_recovers(), ObservingExtension (+40 more)

### Community 397 - "Presentation"
Cohesion: 0.20
Nodes (4): MAX_STREAKS, MAX_VERTEX_BYTES, Presentation, random()

### Community 398 - "avatars/character.rs"
Cohesion: 0.19
Nodes (9): GROUPS, JOINTS, MATERIALS, STYLES, Vertex, AvatarMesh, AvatarVertex, emit_cuboid() (+1 more)

### Community 399 - "prepare_recovery"
Cohesion: 0.33
Nodes (6): Checkpoint, invalid(), prepare_recovery(), read(), replay(), save()

### Community 400 - "extension_tests.rs"
Cohesion: 0.30
Nodes (6): edit(), external_storage_lifecycle_seam_restart_retries_and_exact_refunds(), ids(), open(), resident(), startup()

### Community 401 - "preview/perf.rs"
Cohesion: 0.27
Nodes (6): GPU_READBACK_TIMEOUT, PerfGpuMesh, PerfGpuSubmesh, PerfPhase, PerfSample, print_percentiles()

### Community 402 - "streaming.rs"
Cohesion: 0.09
Nodes (19): OutboundClientSnapshot, can_stream_snapshot(), can_stream_snapshot_size(), inside_view(), apply(), disconnect(), Input, prepare() (+11 more)

### Community 403 - "host-api/src/content.rs"
Cohesion: 0.11
Nodes (15): Block, BlockState, DropAnimation, .BYTE_LEN, FaceTextures, Geometry, Material, Property (+7 more)

### Community 404 - "declarer"
Cohesion: 0.19
Nodes (4): Declaration, declarer(), field(), parse_recipe()

### Community 406 - "notifications.rs"
Cohesion: 0.22
Nodes (5): Event, Lane, MAX_EVENT_BYTES, QUEUE, view_entity()

### Community 407 - "render/effects/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_preview(), SHADER, verified_example_gpu_pass_survives_resize_and_grades_scene(), version_two_graph_composes_declared_inputs_and_parameters_on_gpu()

### Community 409 - "mixer/tests.rs"
Cohesion: 0.24
Nodes (10): clip_mixer(), initial_obstruction_muffles_short_one_shot_from_its_first_attack(), malformed_obstruction_cannot_poison_audio_or_change_playback(), obstruction_glides_without_restarting_a_moving_loop_and_restores_open_path(), obstruction_is_partition_independent_and_ignores_stopped_or_nonpositional_voices(), obstruction_reduces_energy_and_high_frequencies_in_production_mixer(), signal_clip(), signal_clip_frames() (+2 more)

### Community 410 - "Public dynamic-entity surface"
Cohesion: 0.29
Nodes (7): Behavior and movement, Bounds and remaining surfaces, External proof: Copperling, Interaction and presentation, Public dynamic-entity surface, Registration and identity, Verification

### Community 411 - "Bindings"
Cohesion: 0.14
Nodes (13): Action, allowed_key(), Bindings, letter(), NamedBindings, parse(), valid_action_key(), Binding (+5 more)

### Community 413 - "first_person/tests.rs"
Cohesion: 0.14
Nodes (3): grip_anchor(), either_tool_swing_brings_its_own_grip_forward(), View

### Community 414 - "publication.rs"
Cohesion: 0.31
Nodes (7): apply_committed_action(), apply_committed_action_inner(), apply_committed_fire_action(), apply_committed_owner_world(), is_owner_publication_key(), publish_committed(), publish_committed_fire_after_world()

### Community 415 - "render"
Cohesion: 0.47
Nodes (4): read_rgba_png(), render(), render_egui_previews(), render_package_egui_previews()

### Community 416 - "Archived plans and audits"
Cohesion: 0.50
Nodes (4): Archived plans and audits, Foundation and interface, Modding, Scripting

### Community 417 - ".decode_reader"
Cohesion: 0.15
Nodes (6): Clip, DECODED_BYTES, MAX_CLIP_FRAMES, MAX_DECODED_BYTES, MAX_FILE_BYTES, Reservation

### Community 418 - "package/manifest.rs"
Cohesion: 0.20
Nodes (8): asset_path(), bounded_path(), identifier(), Manifest, public_path(), SourceSide, valid_path(), valid_version()

### Community 419 - "lifecycle-fixture/src/machine.rs"
Cohesion: 0.20
Nodes (5): Crush, KEY, MARKED_INPUT, REFINED_INPUT, register()

### Community 420 - "items.rs"
Cohesion: 0.30
Nodes (8): only_block_items_are_placeable(), placeable_block(), placeable_block_in(), SAPLING, SEEDS, STICK, valid_item(), valid_item_in()

### Community 421 - "tests/anchored.rs"
Cohesion: 0.36
Nodes (7): owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record(), owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal(), removed(), seed_storage(), storage_startup(), STORE, unchanged()

### Community 422 - "apply_latest_controls"
Cohesion: 0.38
Nodes (8): apply_latest_controls(), audio_output_callback_converts_channels_and_silences_underruns(), audio_output_controls_are_coherent_bounded_and_queue_reset_is_guaranteed(), audio_output_reset_adoption_restores_explicit_post_reset_preview_controls(), audio_output_reset_discards_buffered_old_session_and_shutdown_is_silent(), controls(), latest_rain_tuning_survives_full_event_queue_and_reset(), unpack_controls()

### Community 424 - "avatars/tests.rs"
Cohesion: 0.17
Nodes (12): catalog(), gpu_rigid_moving_model_rotates_in_three_dimensions_without_creature_deformation(), gpu_moving_projectile_flight_bounce_guidance_and_impact_filmstrip(), authored_gpu_character_draws_textured_animated_geometry_and_instance_tint(), both_bodies_all_hairstyles_support_independent_rgb_without_neighbor_changes(), different_recipes_color_only_selected_irises_and_swap_hair_per_instance(), gpu_registered_player_palettes_preserve_default_and_color_all_three_parts(), HEIGHT (+4 more)

### Community 425 - "server/weather/tests.rs"
Cohesion: 0.33
Nodes (7): captured_weather_is_stable_and_override_invalidates_admitted_reads(), durable_apply_does_not_rewind_elapsed_weather_time(), natural_target_transitions_notify_once_even_when_the_target_kind_repeats(), real_listener_synchronizes_admin_weather_and_denies_other_players(), severity_transitions_remain_continuous_and_survive_checkpoint(), temporary(), weather_restart_preserves_transition_and_wal_recovers_unapplied_override()

### Community 427 - "entities/moving_tests.rs"
Cohesion: 0.26
Nodes (7): catalog(), control_only_publications_advance_full_motion_without_advancing_wire_position_revision(), motion_only_replica_commits_allow_envelope_changes_and_preserve_public_state_revision(), moving_projection_validates_redundant_wire_pose_and_exposes_only_authored_public_bytes(), projected(), State, unchanged_motion_revision_cannot_change_pose_or_other_envelope_fields()

### Community 428 - "TileKey"
Cohesion: 0.14
Nodes (6): TileKey, build(), builtin_lod_tile(), coarse_tiles_are_deterministic_and_dark_beneath_top_surface(), routine_distant_skyline_tiles_fit_payload_budget(), World

### Community 429 - "reaction_removal_tests.rs"
Cohesion: 0.14
Nodes (6): drops(), open_soil(), RemovalDecision, SoilPost, SoilRegistration, stage_reaction()

### Community 430 - "custom/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry(), gpu_preview(), gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters(), GREEN

### Community 431 - "General anchored block entities"
Cohesion: 0.40
Nodes (5): Callback, Declaration, Example and checks, General anchored block entities, Scheduling, authority and publication

### Community 433 - "Cross-cutting integration findings"
Cohesion: 0.13
Nodes (15): Approved unified gameplay implementation, Audit boundary and live path, Baseline and verdict, Built-in capability parity audit, Cross-cutting integration findings, Evidence and integration acceptance, Existing capabilities versus new gameplay, F1 — Registered actions must replace the still-live kiln shortcut (+7 more)

### Community 434 - "render_async"
Cohesion: 0.23
Nodes (5): Blend, Options, read_bounded(), render(), render_async()

### Community 435 - "quad"
Cohesion: 0.43
Nodes (3): linear_color(), quad(), Session

### Community 436 - "Client audio foundation"
Cohesion: 0.15
Nodes (13): Acceptance measurements, Client audio foundation, Mixer and native playback contract, NoiseMachine adaptation, Open shelter entrances, Package acoustic authoring, Packaged scripting extension, Positional clip obstruction (+5 more)

### Community 437 - "Payload"
Cohesion: 0.15
Nodes (3): Payload, host_rejects_ignored_out_of_range_reads_unchecked_motion_and_invalid_state(), Invalid

### Community 439 - "slots"
Cohesion: 0.40
Nodes (4): apply(), capture(), slots(), stack()

### Community 440 - "Control"
Cohesion: 0.28
Nodes (3): Control, control_values_preserve_byte_limits_ranges_and_declared_choices(), SelectOption

### Community 441 - "stack"
Cohesion: 0.16
Nodes (9): advance(), chest_hopper_chest_chain_preserves_last_slot_components_restart_and_refunds(), contents(), resident(), transfer(), hopper(), pulse(), apply() (+1 more)

### Community 442 - "coder-smart.md"
Cohesion: 0.33
Nodes (5): Handoff, Implementation standard, Shared tree and commits, Start and scope, Verification

### Community 443 - "Phase 8: examples, parity and integrated verification"
Cohesion: 0.33
Nodes (6): Authoring and runnable examples, Integrated behavior and responsiveness, Phase 8: examples, parity and integrated verification, Production parity audit, Reproduce verification, Visual inspection and rendering context

### Community 444 - ".local_pose"
Cohesion: 0.21
Nodes (4): Channel, Clip, Model, Transform

### Community 445 - "raycast_blocks"
Cohesion: 0.22
Nodes (7): aiming_past_grass_edges_reaches_ground_but_center_hits_flower(), integer_plane_moving_negative_starts_in_the_entered_voxel(), parallel_axis_uses_half_open_boundary_ownership(), raycast_blocks(), reports_target_face_and_adjacent_cell(), simultaneous_corner_crossing_advances_all_axes(), traverses_negative_coordinates_and_negative_faces()

### Community 446 - "Agent guidance"
Cohesion: 0.50
Nodes (4): Agent guidance, Architecture and invariants, graphify, Verify graphics and performance

### Community 447 - "startup/moving/tests.rs"
Cohesion: 0.16
Nodes (4): Fixture, moving_decimal_minimum_extent_matches_native_f32_validation(), moving_startup_collects_frozen_body_and_three_handlers(), moving_startup_supports_private_model_free_entities()

### Community 448 - "manifest.json"
Cohesion: 0.22
Nodes (8): head_joint, joints, records, runtime_forward, runtime_scale, source_forward, source_height, version

### Community 449 - "runtime/memory.rs"
Cohesion: 0.29
Nodes (8): begin(), exceeded(), install(), memory_error(), memory_text(), observe(), reject(), Rejected

### Community 452 - "resources.rs"
Cohesion: 0.20
Nodes (8): ArrayUsage, MAX_ARRAY_BYTES, MAX_ARRAY_LAYERS, required_limits(), counts_every_mipmap_and_enforces_the_array_byte_boundary(), requests_enough_device_layers_for_package_textures_and_native_materials(), target_package_texture_count_builds_a_valid_gpu_material_array(), validate()

### Community 453 - "set_preview_block"
Cohesion: 0.21
Nodes (5): avatars(), fixture_samples_distinguish_sky_portal_sealed_and_emissive_light(), prepare(), Scene, set_preview_block()

### Community 457 - "TickSample"
Cohesion: 0.28
Nodes (3): TickSample, longest_backlog_streak(), mean_pending()

### Community 458 - "FireAnimator"
Cohesion: 0.23
Nodes (4): FireAnimator, LIFE, MAX_DISTANCE_SQUARED, MAX_FIRES

### Community 459 - "obstruction_state/tests.rs"
Cohesion: 0.31
Nodes (8): capture(), chunks(), late_session_results_cannot_consume_the_new_pending_capture(), missing_result_times_out_and_resubmits_without_wedging_playback(), playing(), queue_pressure_retries_but_an_edit_or_teleport_discards_the_result(), real_worker_delivers_initial_profile_and_refreshes_after_wall_edit(), snapshot_fences_same_revision_replacements_installations_and_evictions()

### Community 461 - "src/storage.rs"
Cohesion: 0.14
Nodes (11): CONTENT_MAP, CONVERSION_INCOMPLETE, FORMAT_VERSION, HEADER_LEN, MAGIC, MAX_SNAPSHOT_BYTES, SAVE_FORMAT_VERSION, TEMP_COUNTER (+3 more)

### Community 462 - "prepare_recovery"
Cohesion: 0.35
Nodes (7): decode_anchor(), invalid(), prepare_recovery(), read(), replay(), save(), Snapshot

### Community 465 - "bloxgloom"
Cohesion: 1.00
Nodes (3): bloxgloom, bloxgloom-host-api, bloxgloom-lifecycle-fixture

### Community 504 - "palette/tests.rs"
Cohesion: 0.43
Nodes (4): full_palette_reuses_removed_entry_and_preserves_other_cells(), row_copy_matches_flat_storage_in_each_mode(), state(), uniform_and_local_palette_boundaries_round_trip()

### Community 505 - "CommitAction"
Cohesion: 0.13
Nodes (12): CommitAction, plan_motion(), builtin(), cue(), clamp(), commit(), expire(), invalid() (+4 more)

### Community 507 - "declarer"
Cohesion: 0.23
Nodes (4): declarer(), field(), number(), triple()

### Community 508 - "Procedural"
Cohesion: 0.07
Nodes (12): Preset, play_file(), play_preview(), render(), render_insect_preview(), render_material_preview(), render_preview(), validate_seconds() (+4 more)

### Community 509 - "duration"
Cohesion: 0.10
Nodes (11): authored_motion_uses_server_age_and_continues_into_partial_pickup(), item(), moving_drop_blends_between_authoritative_positions(), sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion(), app(), avatar(), breaking_starts_a_bounded_tool_animation_and_stance_requires_server_confirmation(), held_break_cancels_on_menus_capture_loss_and_session_retirement() (+3 more)

### Community 510 - "declarer"
Cohesion: 0.14
Nodes (4): articulated_recipes_replicate_independently_and_survive_server_restart(), Peer, declarer(), dense_array()

### Community 511 - "visual_contracts.rs"
Cohesion: 0.29
Nodes (7): copy(), fixture(), installed_authoritative_entity_replica_drives_the_shader_parameter(), invalid_typed_startup_parameter_refuses_real_join_with_source_identity(), negotiated_visuals_parameters_switch_and_restart_without_global_state(), open_boxed(), scalar()

### Community 513 - "render.rs"
Cohesion: 0.08
Nodes (11): avatar(), codec_normalization_tolerance_cannot_break_quaternion_interpolation(), committed_ticks_interpolate_orientation_without_gait_or_extrapolation(), impact_corrects_immediately_and_stale_motion_cannot_resurrect_flight(), model_replacement_and_removal_drop_retained_motion_history(), DEPTH_FORMAT, MAX_PENDING_MESHES, SUN_DIRECTION (+3 more)

### Community 514 - "tests/client.rs"
Cohesion: 0.39
Nodes (7): asset_aggregate_bytes_and_declaration_count_are_bounded(), asset_set_count_is_bounded_even_for_empty_files(), assets_use_secure_bounded_regular_file_reads(), classified(), classified_packages_preserve_clientless_startup_and_server_import_authority(), discovery_exports_only_classified_frozen_bytes_in_canonical_order(), export_paths_types_and_sides_fail_closed()

### Community 515 - "admin/tests.rs"
Cohesion: 0.12
Nodes (5): declared_aliases_and_quoted_text_use_canonical_typed_requests(), generic_parser_uses_ordered_negotiated_schema_not_builtin_names(), player_command_names_completion_and_reconnect_use_exact_sessions(), request(), command_signature()

### Community 516 - "Startup"
Cohesion: 0.07
Nodes (3): ClientBundle, Format, Startup

### Community 517 - "model_asset/tests.rs"
Cohesion: 0.24
Nodes (6): atlas_color_controls_can_target_separate_parts_and_reject_overlap(), controls(), Fixture, native_glb_decodes_embedded_png_and_weighted_skins(), native_glb_preserves_named_clips_and_customizable_subtrees(), native_glb_rejects_bad_accessors_cycles_and_external_resources()

### Community 518 - "Acoustics"
Cohesion: 0.24
Nodes (3): Acoustics, Habitat, RainSurface

### Community 520 - "client_metadata.rs"
Cohesion: 0.23
Nodes (4): Catalog, Entity, Identity, Metadata

### Community 521 - "PlayerDecision"
Cohesion: 0.09
Nodes (7): invoke(), PlayerDecision, admit(), invoke(), profile_state(), deadline(), prepare()

### Community 522 - "Luau runtime tools"
Cohesion: 0.40
Nodes (5): Deterministic randomness, Diagnostics, Examples and compatibility, Libraries, Luau runtime tools

### Community 523 - "plan_observed_request"
Cohesion: 0.33
Nodes (5): denied(), encode_command_arguments(), plan(), plan_observed_request(), plan_request()

### Community 524 - "Larger Luau packages and independent simulation features"
Cohesion: 0.22
Nodes (9): Deterministic terrain contributors, Example and acceptance, Final package measurements, Graphics verification and controls, Independent systems, Larger Luau packages and independent simulation features, Memory and decoded-resource admission, Shared admission policy (+1 more)

### Community 525 - "reads/tests.rs"
Cohesion: 0.39
Nodes (4): captured_reads_are_readonly_sorted_and_paginated(), caught_unknown_tags_and_excess_queries_reject_callback(), Content, environment()

### Community 526 - "script_startup/gameplay/entities.rs"
Cohesion: 0.21
Nodes (7): Fixture, luau_entity_schema_loopback_spawn_due_callback_and_recovery(), luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling(), prepare(), REGISTER, REGISTER_ENTITY, SOURCE

### Community 529 - "item_visuals/tests.rs"
Cohesion: 0.29
Nodes (7): example(), item_visuals_bundle_rejects_nonfinite_icon_payloads_and_missing_items(), item_visuals_delivered_icons_and_stack_callbacks_preserve_catalog_and_inventory(), item_visuals_drop_animator_applies_only_client_presentation_scale(), item_visuals_drop_animator_consumes_received_components_for_live_and_pickup_art(), item_visuals_readonly_inputs_bounded_replies_and_infinite_callbacks_fail(), wait()

### Community 531 - "Player lifecycle implementation"
Cohesion: 0.17
Nodes (12): Landed: committed observers, Landed: exact identity and action queries, Landed: general package-owned profile state, Landed: lifecycle state and scheduling, Landed: local public state and client lifecycle, Landed: runtime appearance, Landed: runtime teleport, Landed: targeted notices and session kicks (+4 more)

### Community 536 - "rain/tests.rs"
Cohesion: 0.26
Nodes (7): active_drop_turns_with_listener_without_restarting_its_tail(), changing_spatial_hearing_retargets_sounding_drops_without_respawning(), drop(), drop_volume_and_reverb_are_independent_of_the_diffuse_bed(), rain_bubble_matches_physical_frequency_damping_delay_and_gain(), rain_dense_pool_rejects_overload_and_reclaims_every_tail(), rain_invalid_inputs_reject_without_mutating_configuration()

### Community 540 - "shadow_tests.rs"
Cohesion: 0.16
Nodes (7): avatar(), gpu_avatar_receivers_remove_only_direct_sun_and_off_matches_unoccluded(), gpu_character_casters_keep_full_world_rig_and_selected_hair_in_first_person(), gpu_public_casters_share_creature_and_rigid_animated_geometry(), HEIGHT, Scene, WIDTH

### Community 541 - "spawn.rs"
Cohesion: 0.47
Nodes (5): collides(), collides_cached(), request_missing(), spawn_position(), spawn_position_cached()

### Community 542 - "tcp/report.rs"
Cohesion: 0.31
Nodes (5): TransportSnapshot, OutboundSnapshot, percentile(), summarize(), TcpSoakReport

### Community 544 - "Harvest"
Cohesion: 0.29
Nodes (3): Harvest, Pickup, PlantSupport

### Community 545 - "position_store/tests.rs"
Cohesion: 0.21
Nodes (5): corrupted_position_is_not_silently_replaced(), position_checkpoint_does_not_share_inventory_temp_namespace(), position_round_trips_and_is_profile_scoped(), SEQUENCE, TestSave

### Community 546 - "Modding: start here"
Cohesion: 0.50
Nodes (4): Historical design and audit, Modding: start here, Rust extension and host reference, Try authoring now

### Community 547 - "Config"
Cohesion: 0.07
Nodes (30): clamp_finite(), Config, CONFIG_VERSION, create_temporary_file(), MAX_FOV, MAX_SCALE, MAX_SENSITIVITY, MIN_FOV (+22 more)

### Community 549 - "mlua"
Cohesion: 0.06
Nodes (12): Tags, declarer(), FixedBytes, NEXT_REALM, guarded(), install_tags(), invalid(), members() (+4 more)

### Community 555 - "inventory"
Cohesion: 0.13
Nodes (6): Codec, ContainerPayload, register(), settle(), text(), typed_recipe_browser_uses_exact_inventory_components_clock_and_own_receipts()

### Community 557 - "Storm"
Cohesion: 0.14
Nodes (8): bell(), Cell, front(), mix(), smooth(), Storm, Weather, Preview

### Community 558 - "authored-model/generate.py"
Cohesion: 0.12
Nodes (15): Bounds, atomicity and compatibility, Composition and deterministic resolution, Existing capabilities exposed, Integration additions, Luau package capacities, Registered content and composition, Verification and limits, accessor() (+7 more)

### Community 559 - "axis"
Cohesion: 0.35
Nodes (5): axis(), decode(), number(), position(), text()

### Community 560 - "server/effects/tests.rs"
Cohesion: 0.32
Nodes (9): RoutedEffect, block_changes_fan_out_to_boundary_face_edge_and_corner_owners(), cell_effects_route_through_chunk_boundaries_and_euclidean_negative_coordinates(), chunk(), effects_for(), envelope(), local_and_cross_chunk_effects_share_the_interaction_commit_barrier_and_stable_order(), output_overflow_is_explicit_and_rejects_the_whole_producer_buffer() (+1 more)

### Community 561 - "pack"
Cohesion: 0.27
Nodes (4): pack(), Part, Source, material_batches_preserve_joint_and_tint_ids_and_skip_hidden_variants()

### Community 562 - "modding/README.md"
Cohesion: 0.06
Nodes (31): Growth foundation: remaining implementation, Vegetation rules, Typed commands and aliases, Chunk generation for native contributors, Luau contributor composition, Luau item icons and stack presentation, Authored UI widgets and current limits, Local Luau packages (+23 more)

### Community 563 - "bounded.rs"
Cohesion: 0.18
Nodes (6): expiry_due_prefix_is_capped_and_uncommitted_work_remains_eligible(), fill_chunk(), motion_is_scheduled_entity_work_never_coordinator_stepping(), one_moving_drop_costs_its_own_records_never_the_population(), staged_motion_bytes(), test_store()

### Community 564 - "declarations/budget.rs"
Cohesion: 0.25
Nodes (5): block_bytes(), MAX_BYTES, MAX_COUNT, package_bytes(), tag_bytes()

### Community 566 - "invalid"
Cohesion: 0.07
Nodes (20): decode(), encode(), decode(), encode(), float(), decode(), encode(), Format (+12 more)

### Community 567 - "plan_event"
Cohesion: 0.50
Nodes (3): is_registered(), plan(), plan_event()

### Community 568 - "seed"
Cohesion: 0.26
Nodes (8): entity(), hex_id(), partition(), present(), profile(), seed(), word(), words()

### Community 569 - "memory/tests.rs"
Cohesion: 0.48
Nodes (5): memory_errors_latch_before_handlers_transform_them_and_reset_explicitly(), protected_calls_can_yield_and_resume_without_a_rust_boundary(), protected_calls_preserve_values_and_normal_errors(), real_allocator_error_caught_in_pcall_is_latched(), runtime()

### Community 573 - "gameplay"
Cohesion: 0.14
Nodes (3): box_cells(), MAX_QUERY_CELLS, apply()

### Community 577 - "voices/tests.rs"
Cohesion: 0.29
Nodes (8): event(), moving_pending_entity_updates_capture_without_native_update_and_rejects_old_voxel(), pending_one_shot_expiry_begins_at_atomic_native_admission(), pending_profile_retries_without_starting_or_expiring_when_native_queue_is_full(), pending_start_falls_back_after_100ms_and_pending_changes_are_used_at_admission(), play(), state(), stopping_pending_voice_emits_no_native_stop_and_late_profile_cannot_resurrect_it()

### Community 578 - "Authoritative moving entities"
Cohesion: 0.14
Nodes (11): Acceptance evidence, Admission limits, Authoritative moving entities, Behavior events, Compatibility and presentation, Declaration, Gameplay services, Integration, reactions and persistence (+3 more)

### Community 582 - "CharacterAsset"
Cohesion: 0.10
Nodes (6): CharacterAsset, CharacterVertex, Joint, JOINT_COUNT, MATERIAL_COUNT, COMPUTE_FIXTURE

### Community 591 - "parse"
Cohesion: 0.10
Nodes (11): decode(), field(), parse(), records(), parse(), transfer(), decode(), fields() (+3 more)

### Community 593 - "AUTHORED-MODELS.md"
Cohesion: 0.10
Nodes (16): Authored clips, Controls and previews, Export and structure, Revised Blockbench character master, Appearance, Geometry and movement, Native GLB player, Save and wire boundary (+8 more)

### Community 594 - "owner_commit/tests.rs"
Cohesion: 0.08
Nodes (24): a_system_wave_cannot_commit_two_patches_for_the_same_owner(), batch(), chunk(), handler_or_budget_failure_aborts_the_entire_wave(), patch(), results(), system(), validated_wave_applies_in_canonical_owner_order_after_aggregate_checks() (+16 more)

### Community 596 - "hierarchy"
Cohesion: 0.15
Nodes (4): hierarchy(), load(), validate(), visit()

### Community 597 - "tests/materials.rs"
Cohesion: 0.29
Nodes (7): bound_texture_asset_must_decode_and_match_owned_canonical_metadata(), bundle(), DESCRIPTOR, material_bundle_verifies_key_ownership_limits_and_catalog_readiness(), SHADER, texture_metadata(), version_two_material_verifies_hooks_parameters_and_negotiated_layers()

### Community 599 - "tests/appearance.rs"
Cohesion: 0.29
Nodes (6): mismatched_player_catalog_preserves_existing_world_and_profile_files(), old_profile_formats_are_rejected_without_resetting_or_rewriting_them(), profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default(), profile_appearance_save_failure_never_publishes_and_stops_mutation(), recipe_save_failure_never_publishes_and_invalid_selection_never_writes(), recipe_save_is_profile_bound_canonical_and_exposes_only_legacy_host_projection()

### Community 600 - "presentation/observations/tests.rs"
Cohesion: 0.48
Nodes (5): committed_spawn_ordinals_expose_exact_readonly_entity_handles(), known(), observations_distinguish_unknown_from_known_empty_and_filter_action_ownership(), observations_keep_exact_binary_revisions_and_nested_views_readonly(), observations_reject_invalid_dense_inventory_components_world_and_window_bounds()

### Community 601 - "SpawnReceipt"
Cohesion: 0.44
Nodes (6): SpawnReceipt, MAX_ACTION_SPAWNS, read(), read_bytes(), validate(), write()

### Community 602 - "DropPolicy"
Cohesion: 0.14
Nodes (8): DropPolicy, .BYTE_LEN, .MAX_PICKUP_RANGE, DropSize, component_schema(), drop_policy(), options(), hexadecimal_schema_fingerprint_is_exact_and_cannot_mix_encodings()

### Community 604 - "import_napp.py"
Cohesion: 0.52
Nodes (4): import_natural(), main(), resized_data(), tinted()

### Community 606 - "outbound/tests.rs"
Cohesion: 0.43
Nodes (6): aggregate_byte_limit_is_enforced_across_clients(), aggregate_high_water_mark_survives_sub_tick_queue_drain(), frame_admission_includes_the_frame_currently_being_written(), per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop(), pong(), shared_encoding_keeps_independent_byte_reservations_until_each_client_releases()

### Community 608 - "commands.rs"
Cohesion: 0.23
Nodes (8): advertised_builtin_commands_and_compatibility_packets_share_auth_receipts_and_restart(), command_request(), declaration(), extended_commands_negotiate_aliases_and_reject_forged_values_before_durable_dispatch(), invalid_command_declarations_poison_startup_even_when_caught(), negotiated_commands_enforce_permission_and_zero_args_with_receipts_and_restart(), typed_mod_command_negotiates_order_validates_before_handler_and_recovers_once(), typed_request()

### Community 609 - "client/appearance.rs"
Cohesion: 0.43
Nodes (3): apply_environment(), invalid(), parse()

### Community 611 - "client/observations/tests.rs"
Cohesion: 0.35
Nodes (8): app(), inventory_zero_is_known_and_stale_or_duplicate_updates_do_not_replace_it(), only_contiguous_installed_deltas_publish_authoritative_cells(), pending(), receipts_retain_package_ownership_deduplicate_and_retire_with_the_session(), recent_cells_are_bounded_refreshed_by_snapshots_and_removed_on_eviction(), terrain_action_receipts_preserve_the_registered_package_key(), ui_less_callbacks_receive_latest_world_snapshot_as_readonly_data()

### Community 613 - "script_startup/player.rs"
Cohesion: 0.26
Nodes (8): check_movement(), custom_player_rules_negotiate_before_welcome_and_survive_restart(), FIELDS, player_artifact_tampering_is_rejected_or_fails_exact_manifest_match(), player_rules_and_drop_animation_share_one_verified_bundle_before_join(), player_rules_compose_with_existing_sized_item_artifacts(), rejected_player_declarations_including_caught_errors_never_open_world(), source()

### Community 615 - "bundle_ui.rs"
Cohesion: 0.13
Nodes (12): event(), client_startup_failure_refuses_content_ready_with_package_and_module(), downloaded_client_startup_is_session_scoped_across_reconnect_and_switch(), downloaded_replica_visuals_use_exact_entity_ids_and_reset_on_switch(), startup_fixture(), startup_worker_discards_partial_registration_and_caught_limit(), startup_worker_imports_exact_direct_dependencies_with_lexical_visibility(), verified_replica_callbacks_are_session_scoped_worker_presentations() (+4 more)

### Community 616 - "complete_barrier"
Cohesion: 0.18
Nodes (11): advance_receipts(), CommitProgress, complete_barrier(), drain_staged_receipts(), flush_ready_fire(), poll_journal_receipts(), caught_invalid_burn_removal_decision_rejects_whole_owner_wave(), luau_burn_removal_context_and_drop_commit_with_owner_receipt() (+3 more)

### Community 617 - "PublicEntity"
Cohesion: 0.09
Nodes (28): Replicas, BlockCellChange, enforce_frame_size(), MAX_BLOCK_CHANGES_PER_PART, MAX_ENTITIES_PER_PAGE, MAX_ENTITY_CHANGES_PER_PART, MAX_ENTITY_SNAPSHOT_BYTES, MAX_ENTITY_SNAPSHOT_PAGES (+20 more)

### Community 619 - "actors/tests.rs"
Cohesion: 0.35
Nodes (10): avatar(), crouch_and_head_pitch_ease_and_teleport_resets_the_presentation_history(), fast_movement_cannot_accelerate_authored_walk_past_normal_playback(), ground_speed_blends_walk_and_run_but_stale_or_airborne_motion_decays(), interpolation_moves_between_samples_and_freezes_without_extrapolation(), landing_animation_follows_delayed_ground_contact_and_is_visual_only(), player_walk_blends_from_replicated_distance_then_stops_without_drift(), predicted_local_player_is_not_delayed_and_faces_the_current_look_heading() (+2 more)

### Community 623 - "Luau VM lifetime and module state"
Cohesion: 0.29
Nodes (7): Choose the right state, Contexts and coroutines, Failures and teardown, Initialization and random streams, Luau VM lifetime and module state, Runnable example and measurement, Verification

### Community 624 - "exposure"
Cohesion: 0.25
Nodes (4): exposure(), hillside(), hillside_opening_produces_audible_rain_through_the_game_mixer(), open_hillside_entrance_is_audible_and_closing_it_restores_muffling()

### Community 626 - "script_startup/gameplay/profile_state.rs"
Cohesion: 0.19
Nodes (9): fixture(), lifecycle_profile_writes_compose_and_ambiguous_decisions_rollback_over_listener(), OFFLINE, profile_state_actions_are_atomic_owned_binary_and_restart_safe_over_listener(), profile_state_reads_reserve_existing_cells_and_absence_until_receipt(), profile_state_service_requires_package_capability_over_listener(), read_cell(), publish() (+1 more)

### Community 628 - "server/lod/tests.rs"
Cohesion: 0.16
Nodes (10): cancelled_builds_retire_every_budget_slot_and_shutdown_with_full_completion_queue(), new_authoritative_high_load_invalidates_cached_partial_contributor_summary(), observed_world(), ObservedHighTerrain, resident_capture_budget_rejects_excessive_observed_columns_and_ignores_builtin_high_air(), resident_high_contributor_coverage_survives_eviction_without_save_data_or_pins(), revision_exhaustion_cancels_every_inflight_dependency(), temporary() (+2 more)

### Community 629 - "decode"
Cohesion: 0.31
Nodes (6): axes(), decode(), encode(), MAGIC, word(), wrap()

### Community 630 - "anchored_tests.rs"
Cohesion: 0.22
Nodes (11): anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic(), command(), edit(), fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record(), KEY, open(), public(), resident() (+3 more)

### Community 631 - "weather/codec.rs"
Cohesion: 0.33
Nodes (3): BYTES, decode(), encode()

### Community 633 - "Reaction"
Cohesion: 0.33
Nodes (5): Reaction, bytes(), invalid(), parse(), Reply

### Community 634 - "protocol/lod.rs"
Cohesion: 0.18
Nodes (10): invalidation_rejects_old_builds_but_keeps_displayed_replicas(), obsolete_requests_and_other_sessions_cannot_install_tiles(), state(), teleport_reset_cancels_requests_without_reusing_request_identity(), tile(), read_key(), read_tile(), tile_len() (+2 more)

### Community 635 - "entities"
Cohesion: 0.14
Nodes (7): unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers(), registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes(), live_command(), missing_mossbun_terrain_defers_locally_then_runs_on_residency(), mossbun_authorized_spawn_worker_steps_and_restart_preserve_identity(), mossbun_spawn_limit_rejects_without_allocating_or_consuming_items(), resident_platform()

### Community 636 - "colliders/tests.rs"
Cohesion: 0.27
Nodes (4): Bytes, declaration(), moving_capture_uses_committed_creature_crossing_and_fences_target_motion(), record()

### Community 640 - "sun_shadow/tests.rs"
Cohesion: 0.39
Nodes (5): camera(), grazing_sun_shadow_strength_fades_continuously(), offscreen_nearby_occluders_remain_in_shadow_frustum(), projections_are_finite_at_noon_horizon_and_zenith_and_disable_at_night(), sub_texel_camera_motion_keeps_world_shadow_projection_stable()

### Community 641 - "visibility.rs"
Cohesion: 0.43
Nodes (4): chunk_visible(), chunk_visible_padded(), outside_clip(), view_projection()

### Community 642 - "Packaged and scripted audio"
Cohesion: 0.50
Nodes (4): Client presentation commands, Packaged and scripted audio, Server gameplay calls, Verification — October 1, 2026

### Community 644 - "Game weather foundation"
Cohesion: 0.20
Nodes (10): Acceptance evidence, Authority, timing and persistence, Depth-fog acceptance, Game weather foundation, Integration with articulated characters, Luau weather services, Material and insect sound integration, Presentation and bounds (+2 more)

### Community 646 - "tests/effects.rs"
Cohesion: 0.33
Nodes (4): bundle(), DESCRIPTOR, SHADER, verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures()

### Community 648 - "EntityDefinition"
Cohesion: 0.10
Nodes (5): EntityDefinition, EntityState, Context<'_>, Weather, WeatherKind

### Community 748 - ".sample_gameplay_look"
Cohesion: 0.27
Nodes (6): blend(), CharacterAsset, overlay(), mirrored_tools_move_both_elbows_and_wrists_and_return_without_a_pop(), tool_duration(), weight()

### Community 749 - "Receiver"
Cohesion: 0.24
Nodes (5): Receiver, CheckpointWork, apply_motion(), run(), run_loop()

### Community 750 - "Articulated renderer performance"
Cohesion: 0.50
Nodes (3): Articulated renderer performance, Measurements, Terrain-only comparison

### Community 751 - "mpsc"
Cohesion: 0.18
Nodes (4): Observer, WeatherChanged, luau_weather_reads_controls_and_hooks_follow_admin_commit_and_restart(), Witness

### Community 752 - "metadata"
Cohesion: 0.47
Nodes (3): metadata(), observer_metadata_is_inert_bounded_and_rejects_nested_envelopes(), runtime_metadata_rejects_unknown_shapes_and_unbounded_counts_with_valid_digest()

### Community 755 - "entities/motion.rs"
Cohesion: 0.25
Nodes (7): DT, MAX_BODIES, MAX_CHUNK_BODIES, MAX_COLLIDERS, MAX_DYNAMIC_COLLIDERS, MAX_SWEEP_CELLS, STEP_TICKS

### Community 760 - "src/motion/tests.rs"
Cohesion: 0.52
Nodes (6): contact_query_tracks_captured_motion_revision_and_absence(), expiry_roundtrip(), pending_reaction_roundtrip_and_truncations(), public_projection_keeps_private_state_separate(), record(), reject_noncanonical_pose_and_impact_identity()

### Community 762 - "write_frame"
Cohesion: 0.25
Nodes (3): bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix(), byte_budget_stops_turns_and_file_limit_does_not_write_overshoot(), write_frame()

### Community 764 - "prepare"
Cohesion: 0.09
Nodes (4): centre(), prepare(), draw(), quality()

### Community 766 - "decode"
Cohesion: 0.11
Nodes (8): simultaneous_verification_is_bounded_and_failure_releases_admission(), decode(), active_and_retiring_references_prevent_eviction_and_retry(), bundle(), metadata_corruption_after_pressure_does_not_restore_retired_memo(), ordinary_corruption_keeps_cache_and_retry_is_strictly_bounded(), pressure(), unused_cache_is_released_before_one_local_verification_retry()

### Community 767 - "content/moving/tests.rs"
Cohesion: 0.36
Nodes (4): Bytes, declaration(), maximum_authored_state_with_long_pending_contact_fits_durable_envelope(), moving_catalog_bindings_follow_saved_identity_remapping()

### Community 768 - "Definitions, assets, and composition"
Cohesion: 0.22
Nodes (4): Capability matrix, Definitions, assets, and composition, block(), Catalog

### Community 769 - "session_ids/tests.rs"
Cohesion: 0.53
Nodes (4): directory(), session_boot_ranges_preserve_first_ids_and_never_reuse_a_durable_player_reference(), session_reservations_are_serialized_and_corrupt_or_exhausted_counters_are_rejected(), session_server_restart_installs_a_new_range_before_player_admission()

### Community 771 - "setup"
Cohesion: 0.48
Nodes (5): exact_component_preimages_and_output_schema_are_checked_before_consumption(), invalid_transformations_cannot_escape_slots_filters_or_stack_limit(), multiple_consumed_slots_and_outputs_are_all_or_nothing(), setup(), transformation_is_atomic_on_capacity_and_stale_exact_input()

### Community 772 - "world"
Cohesion: 0.06
Nodes (22): BurnFuel, check_seeded_fire_restart(), checkpoint_pressure_admits_a_durable_owner_prefix_without_losing_the_remainder(), dirty_fire_keys_use_one_checkpoint_job_and_revision_fenced_receipt(), drain_wal(), fire_burn_uses_public_removal_and_support_handlers_in_one_receipt(), temp_save(), temp_save_dir() (+14 more)

### Community 773 - "run"
Cohesion: 0.48
Nodes (3): main(), parse_tint(), run()

### Community 774 - "register"
Cohesion: 0.40
Nodes (3): definition(), KEY, register()

### Community 775 - "src/appearance.rs"
Cohesion: 0.22
Nodes (7): BODIES, CHARACTER_RECIPE_BYTES, DEFAULT_HAIR_COLOR, EYES, HAIR, MAX_APPEARANCE_BYTES, MOUTHS

### Community 777 - "content/appearance.rs"
Cohesion: 0.20
Nodes (4): Catalog, PANTS, SHIRTS, SKINS

### Community 778 - "Registered inventory views and screens"
Cohesion: 0.33
Nodes (6): Generic client and server paths, Independent persistence and bounds, Public registration, Registered inventory views and screens, Try the external fixture, Verification

### Community 781 - "Registered actions and composed controls"
Cohesion: 0.40
Nodes (5): Bounded composition, Negotiated commands, Production path and authority, Registered actions and composed controls, Supported targets and effects

### Community 796 - "std"
Cohesion: 0.14
Nodes (3): run(), wal_reservation_accounts_for_queued_frames_before_the_worker_sees_them(), writer_treats_repeated_ids_idempotently_and_rejects_conflicts()

### Community 797 - "app"
Cohesion: 0.47
Nodes (3): app(), declared_input_real_client_queues_once_opens_and_rebinds_without_gameplay(), declared_input_real_client_respects_screens_focus_modifiers_and_scope()

### Community 800 - "AudioOutput"
Cohesion: 0.20
Nodes (4): AudioOutput, pack_controls(), Queued, run()

### Community 802 - "channel"
Cohesion: 0.47
Nodes (3): channel(), keyframe_sampling_clamps_steps_and_preserves_cubic_tangents(), rotations_take_shortest_path_and_cubic_output_stays_normalized()

### Community 807 - "Result"
Cohesion: 0.05
Nodes (12): Context<'_>, Context<'_>, parse(), DeadlineStream, Result, EntityView, WorldSnapshot<'_>, WorldSnapshot<'_> (+4 more)

### Community 808 - "Public storage lifecycle boundary"
Cohesion: 0.33
Nodes (6): Chest integration, Explicit limits / next work, Lifecycle contract, Package boundary, Public storage lifecycle boundary, Verification

### Community 810 - "Sandbox rendering fixtures"
Cohesion: 0.33
Nodes (5): Capture, Sandbox rendering fixtures, Software GLES capture compatibility, Visual checks, What is held constant

### Community 811 - "menus.rs"
Cohesion: 0.30
Nodes (12): actions(), admin(), button(), draw(), EDGE, GOLD, MUTED, PANEL (+4 more)

### Community 812 - "vm_latency.rs"
Cohesion: 0.40
Nodes (3): CALLBACK, report(), vm_lifetime_mixed_listener_latency()

### Community 813 - "state"
Cohesion: 0.39
Nodes (5): default_articulated_preview_preserves_palettes_and_clean_drafts_follow_server_changes(), draft_cancel_apply_echo_and_duplicate_apply_are_distinct(), implicit_default_is_editable_without_a_model_toggle_or_phantom_changes(), state(), unknown_snapshot_and_rejected_draft_cannot_apply_and_disconnect_clears_state()

### Community 815 - "receive_result"
Cohesion: 0.32
Nodes (3): audio_timer_fixture_places_completes_and_reconstructs_its_replica_loop(), packaged_audio_commits_once_and_caught_invalid_audio_rolls_back(), receive_result()

### Community 817 - "world_time/tests.rs"
Cohesion: 0.46
Nodes (5): clock_command_recovers_past_an_older_checkpoint_and_rotated_wal(), local_listener_delivers_shared_world_time_admin_changes_and_recovers_it(), save(), temporary(), world_time_resumes_saved_phase_and_rejects_corrupt_state()

### Community 821 - "Registered anchored behavior"
Cohesion: 0.40
Nodes (5): Authority, scheduling and recovery, Bounds and limits, Public contract, Registered anchored behavior, Verification

### Community 822 - "ObserverRegistration"
Cohesion: 0.12
Nodes (4): CommittedBlock, CommittedEntity, ObserverRegistration, Catalog

### Community 823 - ".attempt"
Cohesion: 0.20
Nodes (4): MAX_REPLANT_CELLS, OWNER_PROBES_PER_TICK, Replanter, stage_error()

### Community 824 - "crate"
Cohesion: 0.07
Nodes (8): animate(), FORMAT, HEIGHT, WIDTH, FIRST, PROFILE, SECOND, THIRD

### Community 825 - "Package shape"
Cohesion: 0.67
Nodes (3): Exact identities and time, Larger packages, Package shape

### Community 826 - "Dynamic authored UI and input"
Cohesion: 0.40
Nodes (5): Callback inputs and atomic updates, Declared input actions, Documents and controls, Dynamic authored UI and input, Server authority and remaining scope

### Community 827 - "server/script/tests.rs"
Cohesion: 0.54
Nodes (6): elapsed_deadline_is_reported_with_module_identity(), input(), instruction_and_source_limits_reject_bad_modules_without_poisoning_worker(), memory_limit_and_syntax_error_are_attributable(), module(), sandbox_excludes_native_io_and_attributes_errors()

### Community 835 - "Typed client replica snapshots"
Cohesion: 0.50
Nodes (4): Accepted implementation scope, Typed client replica snapshots, Update and lifecycle semantics, Verification

### Community 838 - "ScriptSystem"
Cohesion: 0.13
Nodes (3): IntentDelivery, inbox(), ScriptSystem

### Community 842 - "passes"
Cohesion: 0.60
Nodes (3): after_dependencies_use_the_same_cycle_and_missing_checks(), inputs_override_order_and_invalid_graphs_keep_resource_identity(), passes()

### Community 844 - "terrain/tests.rs"
Cohesion: 0.29
Nodes (3): box_reads_preserve_order_and_unknown_cells_poison_the_plan(), call(), World

### Community 846 - "Registered inventory machines"
Cohesion: 0.40
Nodes (5): Contract, Registered inventory machines, Remaining boundaries, Trying the fixture, Verification

### Community 870 - "render/camera/tests.rs"
Cohesion: 0.83
Nodes (3): eye(), perspectives_orbit_the_eye_and_cycle_without_changing_aim(), swept_camera_stops_before_walls_and_handles_close_or_unknown_cells()

### Community 871 - ".frame"
Cohesion: 0.13
Nodes (5): ClientApp, ClientApp, ClientApp, ClientApp, Camera

### Community 874 - "MovementState"
Cohesion: 0.16
Nodes (4): MovementState, broadcast(), clear(), resolve()

## Knowledge Gaps
- **1215 isolated node(s):** `version`, `joints`, `source_height`, `head_joint`, `source_forward` (+1210 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 5101 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **312 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Result` connect `Result` to `VoxelView`, `EntityStore`, `PendingWakeStore`, `super`, `src/preview.rs`, `ScriptAnchored`, `.bind_machine`, `ScriptError`, `server/runtime/tests.rs`, `actions/entity.rs`, `OwnerPatch`, `server/fire/tests.rs`, `server/durable.rs`, `SystemId`, `Rain`, `PackageSnapshot`, `Error`, `protocol.rs`, `Port<P>`, `OutboundFrame`, `StateKey`, `WorldSnapshot`, `MobileProbe`, `receipts.rs`, `entities/types.rs`, `handler.rs`, `Handler`, `Change`, `server_state_with_startup`, `EntityIndexes`, `server.rs`, `Error`, `intent.rs`, `JoinApp`, `journal/rotation.rs`, `BlockActionContext`, `drops/planning.rs`, `src/entity.rs`, `Renderer`, `MobileEntity`, `server/effects.rs`, `protocol/tests.rs`, `bench.rs`, `intent/tests.rs`, `model_asset.rs`, `Replicas`, `journal.rs`, `ServerStartup`, `server/script.rs`, `Registrar`, `client/entities/tests.rs`, `parallel.rs`, `perf/fixture.rs`, `World`, `OwnerEffectPatch`, `handler_declarer`, `server/entities/tests.rs`, `conflict_tests.rs`, `declarer`, `protocol/sounds.rs`, `perf/fire.rs`, `Network`, `reactor.rs`, `HandlerRegistration`, `Appearance`, `InventoryStore`, `entities/player.rs`, `tcp.rs`, `Declarations`, `Diagnostics`, `owner/tests.rs`, `Effect`, `JournalWriter`, `CheckpointWriter`, `EntityCheckpointMirror`, `Connection`, `ClientHandle`, `TickId`, `parse`, `durable/coordinator.rs`, `join_worker.rs`, `durable/state.rs`, `dispatch.rs`, `ClientApp`, `solver.rs`, `ComponentValue`, `ChunkLoader`, `PlayerRules`, `script_startup/system.rs`, `Session`, `Item`, `Clock`, `entities/checkpoint.rs`, `burn.rs`, `TerrainReads`, `StorageBlockEntity`, `HarvestSnapshot`, `snapshots/tests.rs`, `authored/tests.rs`, `RainConfig`, `script/runtime.rs`, `net.rs`, `Gpu`, `InventoryId`, `VisualSession`, `entity_recovery/tests.rs`, `ClientBundle`, `host-api/src/actions.rs`, `startup/tests.rs`, `presentation.rs`, `Behavior`, `Buffer`, `Session`, `GenerationError`, `drops/entity.rs`, `MovingSpawn`, `world/generation.rs`, `publication/commit.rs`, `.prepare_benchmark_frontier_wave`, `Adapter`, `InventoryScreen`, `Texture`, `CacheKey`, `custom/shader.rs`, `PostProcess`, `Decision`, `TransportStats`, `script_startup/creature.rs`, `world/generation/tests.rs`, `Service`, `render/effects.rs`, `Adapter`, `drops/queries.rs`, `client/audio/obstruction.rs`, `world/tests.rs`, `gameplay/decisions.rs`, `item_visuals.rs`, `handles.rs`, `Execution`, `lifecycle-fixture/src/system.rs`, `custom.rs`, `init`, `.owner_systems`, `effects/registered.rs`, `AnchoredBlockEntity`, `voxel_view.rs`, `validate_spawn_volume`, `client/bundle.rs`, `Clock`, `Sender`, `fields_with_command`, `script/gameplay.rs`, `Runtime`, `players/lifecycle.rs`, `Update`, `Adapter`, `machine_component_tests.rs`, `World`, `SystemDescriptor`, `install`, `CoordinatorContext`, `invoke`, `MovingEntity`, `wake.rs`, `Player`, `record.rs`, `ScriptCreature`, `Preparation`, `draw_image`, `host-api/src/machine.rs`, `position_store.rs`, `creature/services.rs`, `native.rs`, `Context<'_>`, `Composition`, `inventory/container.rs`, `render/pipeline.rs`, `KilnPayload`, `server/appearance.rs`, `script_startup.rs`, `Contributor`, `request_chunk`, `authored.rs`, `Context`, `parse`, `Runtime`, `systems/world.rs`, `LodTile`, `EntityTransferPolicy`, `Resources`, `Gpu`, `resolve_nodes`, `Committed`, `client/world.rs`, `registry/tests.rs`, `Hit`, `Imports`, `OwnerApplyReceipt`, `src/lod/tests.rs`, `Chunk`, `entities/motion/tests.rs`, `Observations`, `actions/workstation.rs`, `System`, `ScriptMachine`, `files.rs`, `build_stream`, `protocol/drops.rs`, `EntityCodecError`, `Patrol`, `kiln/codec.rs`, `ItemIcon`, `PlayerEntityStore`, `actions/command.rs`, `time`, `client/startup.rs`, `Inputs`, `gameplay/admin.rs`, `EntityDependencies`, `prepare`, `lod/worker.rs`, `prepare`, `.new`, `BootstrapContract`, `_`, `Worker`, `view`, `declarer`, `io`, `Catalog`, `PlayerState`, `ecology/tests.rs`, `Mesh`, `declarer`, `SignalPost`, `Budget`, `Behavior`, `.register_action`, `EntityError`, `public_systems/motion/tests.rs`, `script_startup/generation.rs`, `Storage`, `Declarations`, `streaming/entities.rs`, `Ignitions`, `client/admin.rs`, `prepare`, `ContentManifest`, `public_systems/tests.rs`, `integer`, `colliders.rs`, `declarer`, `EventRealm`, `widgets.rs`, `.first_solid_top`, `journal/tests.rs`, `ensure`, `.finish`, `declarer`, `PlayerSummary`, `declarer`, `install_sandbox_materials`, `decode`, `GpuPass`, `OwnerData`, `CodecProbe`, `.plan`, `install`, `content`, `resolve_player_movement`, `Attempt`, `MobilePages`, `world_to_chunk`, `prepare_recovery`, `streaming.rs`, `host-api/src/content.rs`, `declarer`, `validate`, `notifications.rs`, `Bindings`, `coordinates`, `publication.rs`, `render`, `.decode_reader`, `package/manifest.rs`, `lifecycle-fixture/src/machine.rs`, `.handle`, `entities/moving_tests.rs`, `TileKey`, `reaction_removal_tests.rs`, `render_async`, `Payload`, `slots`, `stack`, `.local_pose`, `startup/moving/tests.rs`, `runtime/memory.rs`, `generation`, `resources.rs`, `save_and_read`, `Codec`, `Candidate`, `prepare_recovery`, `CommitAction`, `decode`, `declarer`, `Procedural`, `declarer`, `Ceiling`, `render.rs`, `Startup`, `.validate_player_selection`, `client_metadata.rs`, `PlayerDecision`, `plan_observed_request`, `script_startup/gameplay/entities.rs`, `State`, `spawn.rs`, `LifecyclePlan`, `Harvest`, `Config`, `mlua`, `load`, `inventory`, `axis`, `invalid`, `plan_event`, `seed`, `prepare`, `gameplay`, `Replace`, `gameplay_pickup.rs`, `parse`, `owner_commit/tests.rs`, `declarer`, `hierarchy`, `SpawnReceipt`, `DropPolicy`, `Wander`, `client/appearance.rs`, `render_sandbox_previews`, `.begin_window_install`, `complete_barrier`, `PublicEntity`, `.register_inventory_screen`, `DistantMarker`, `script_startup/gameplay/profile_state.rs`, `server/lod/tests.rs`, `decode`, `Reaction`, `protocol/lod.rs`, `colliders/tests.rs`, `admin/weather.rs`, `EntityDefinition`, `Receiver`, `validate_spawn`, `decode`, `write_frame`, `prepare`, `decode`, `content/moving/tests.rs`, `world`, `run`, `register`, `content/appearance.rs`, `ProfileCell`, `parse`, `std`, `Atlas`, `.sound`, `world_time/tests.rs`, `ObserverRegistration`, `.attempt`, `.handle`, `.handle`, `ScriptSystem`, `terrain/tests.rs`, `server/ecology.rs`, `MovementState`?**
  _High betweenness centrality (0.472) - this node is a cross-community bridge._
- **Why does `Built-in capability parity audit` connect `Cross-cutting integration findings` to `Definitions, assets, and composition`, `modding/README.md`?**
  _High betweenness centrality (0.038) - this node is a cross-community bridge._
- **Why does `Audit boundary and live path` connect `Cross-cutting integration findings` to `ServerStartup`, `world_to_chunk`, `Result`, `Replicas`?**
  _High betweenness centrality (0.035) - this node is a cross-community bridge._
- **What connects `version`, `joints`, `source_height` to the rest of the system?**
  _1215 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `VoxelView` be split into smaller, more focused modules?**
  _Cohesion score 0.04232057838123788 - nodes in this community are weakly interconnected._
- **Should `EntityStore` be split into smaller, more focused modules?**
  _Cohesion score 0.06020859671302149 - nodes in this community are weakly interconnected._
- **Should `PendingWakeStore` be split into smaller, more focused modules?**
  _Cohesion score 0.0784313725490196 - nodes in this community are weakly interconnected._