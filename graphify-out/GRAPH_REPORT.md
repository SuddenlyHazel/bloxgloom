# Graph Report - bloxgloom  (2026-10-01)

## Corpus Check
- 1141 files · ~1,797,741 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 54 file(s) not represented in the graph (top: .mesh 15, .glb 13, .wgsl 12)

## Summary
- 15382 nodes · 35900 edges · 831 communities (535 shown, 296 thin omitted)
- Extraction: 95% EXTRACTED · 5% INFERRED · 0% AMBIGUOUS · INFERRED: 1951 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `1d10baa0`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- EntityTypeDescriptor
- EntityStore
- Port<P>
- super
- render_previews
- state_for
- world_to_chunk
- VoxelView
- output.rs
- OwnerData
- server/runtime/tests.rs
- Complete modding implementation proposal
- lifecycle-fixture/src/system.rs
- OwnerKey
- scheduler.rs
- server/durable.rs
- view
- PendingWakeStore
- SystemId
- rain.rs
- PackageSnapshot
- Error
- protocol.rs
- VisualAvatar
- OutboundFrame
- StateKey
- WorldSnapshot
- MobileProbe
- receipts.rs
- RegistrationError
- handler.rs
- Handler
- ui/draw.rs
- SystemRuntime
- server_state_with_startup
- EntityIndexes
- .window_event
- Error
- intent.rs
- JoinApp
- journal/rotation.rs
- JobKey
- public/tests.rs
- BlockActionContext
- drops/planning.rs
- MobileEntity
- Renderer
- UiLayout
- server/fire/tests.rs
- StorageBlockEntity
- server/effects.rs
- protocol/tests.rs
- bench.rs
- intent/tests.rs
- src/client.rs
- Replicas
- Item
- ServerStartup
- script.rs
- Registrar
- client/entities/tests.rs
- server/drops/tests.rs
- parallel.rs
- perf/fixture.rs
- World
- OwnerEffectPatch
- actions/entity.rs
- PalettedBlocks
- server/entities/tests.rs
- conflict_tests.rs
- Contributor
- protocol/sounds.rs
- perf/fire.rs
- ChunkCache
- PackageActionProbe
- PublicEntity
- .handle
- key
- reactor.rs
- content
- .begin_window_install
- src/world.rs
- InventoryStore
- Change
- TickSample
- tcp.rs
- Declarations
- GameUi
- Diagnostics
- owner/tests.rs
- Effect
- JournalWriter
- Authored materials and effects
- CheckpointWriter
- RegisteredEffectIntent
- EntityCheckpointMirror
- Connection
- ClientHandle
- TickId
- Value
- CommitAction
- third_person.rs
- package/client.rs
- voxel_view.rs
- Attempt
- durable/state.rs
- avatars.rs
- dispatch.rs
- menus.rs
- solver.rs
- terrain.rs
- Atmosphere
- ComponentMatch
- ChunkLoader
- Journal
- script_startup/system.rs
- Session
- world/tests.rs
- Clock
- PlayerRules
- entities/checkpoint.rs
- serve
- burn.rs
- TerrainReads
- Registration
- HarvestSnapshot
- snapshots/tests.rs
- authored/tests.rs
- ScriptCreature
- script/runtime.rs
- net.rs
- ScriptSystem
- Plan: built-in/mod capability parity
- Behavior
- InventoryId
- VisualSession
- entity_recovery/tests.rs
- effects/registered.rs
- host-api/src/actions.rs
- startup/tests.rs
- Rng
- presentation.rs
- render/mesh.rs
- sync
- Session
- EntityAdapter
- drops/entity.rs
- MovingSpawn
- world/generation.rs
- publication/commit.rs
- entities/types.rs
- StorageOwner
- io
- InventoryScreen
- render/drops.rs
- Texture
- server.rs
- custom/shader.rs
- InventoryProbe
- PostProcess
- Decision
- Receiver
- script_startup/creature.rs
- Growth foundation plan
- world/generation/tests.rs
- extension_lifecycle.rs
- entity_checkpoint/tests.rs
- render/effects.rs
- server/movement.rs
- view.rs
- drops/queries.rs
- PlayerEntityStore
- Adapter
- gameplay/decisions.rs
- entities/container.rs
- handles.rs
- Execution
- systems/world.rs
- Capture
- custom.rs
- init
- Scripting capabilities for mod developers
- Composition
- Stack
- FirePending
- .bind_machine
- UiRenderer
- AnchoredBlockEntity
- articulated.py
- EntityDefinition
- recovery/tests.rs
- script_startup/gameplay.rs
- client/bundle.rs
- sealed_neighborhood
- Clock
- parallel/tests.rs
- Proposal: one coherent gameplay API
- Sender
- render/material.rs
- Adapter
- script/gameplay.rs
- Runtime
- thunder.rs
- ProfileCell
- players/lifecycle.rs
- Update
- client/startup.rs
- VisualFire
- render/tests.rs
- machine_component_tests.rs
- character_asset.rs
- World
- host-api/src/machine.rs
- SystemDescriptor
- neighborhood.rs
- script_startup/gameplay/profile_state.rs
- CoordinatorContext
- invoke
- MovingEntity
- .register
- wake.rs
- install
- record.rs
- snapshots.rs
- Preparation
- Network
- src/preview.rs
- Ignitions
- position_store.rs
- creature/services.rs
- draw
- Context<'_>
- durable/checkpoint.rs
- bounded.rs
- script_startup/appearance.rs
- pipeline.rs
- WeatherSnapshot
- entity_sleep.rs
- model.rs
- Execution foundation: next implementation slices
- server/appearance.rs
- script_startup.rs
- script_startup/bundle.rs
- ScriptError
- script/generation.rs
- BlockEditCommand
- authored.rs
- src/composition.rs
- Patrol
- Writer
- client/lifecycle/tests.rs
- parse
- Cross-cutting integration findings
- package/tests.rs
- ScriptAnchored
- lighting.rs
- slot.rs
- State
- GenerationError
- EntityTransferPolicy
- Resources
- Gpu
- PlayerSummary
- resolve_nodes
- FootprintCell
- system/intents.rs
- Committed
- client/world.rs
- registry/tests.rs
- Hit
- Imports
- OwnerApplyReceipt
- decode_transaction
- entities/player.rs
- package/manifest.rs
- Observations
- script_startup/anchored.rs
- Declarations
- System
- Scripting Gap Closure Plan
- ScriptMachine
- ReplicationProbe
- Invalid
- files.rs
- entity
- check_articulated_clearance.py
- .public_view
- atomic
- Command
- registered/tests.rs
- tests/client.rs
- runtime/memory.rs
- ItemIcon
- Adapter
- character_asset/gameplay/tests.rs
- Harvest
- Mixer
- .new
- position_store/tests.rs
- declarer
- Inputs
- decode
- script/capacity.rs
- EntityDependencies
- Downloads
- Replace
- SignalPost
- OwnerWorldView
- inventory/container.rs
- Storage
- BootstrapContract
- _
- Resolved
- src/client/tests.rs
- gameplay_anchor_tests.rs
- raycast_blocks
- Payload
- Catalog
- .spawn
- script_startup/gameplay/entities.rs
- render.rs
- declarer
- prepare
- join
- complete_barrier
- build_stream
- .entity_type_id_by_key
- queries/tests.rs
- Player
- .accept
- 2. Player and lifecycle hooks
- EntityError
- actors/tests.rs
- script_startup/generation.rs
- server/gameplay/entities.rs
- Config
- streaming/entities.rs
- MirrorPermit
- src/actions/tests.rs
- Shared<T>
- rig.rs
- tick/tests.rs
- player_services/tests.rs
- Bloxgloom interface plan
- EntityCodecError
- reaction_removal_tests.rs
- nearest_unsent
- tests/anchored.rs
- prepare
- script_startup/player.rs
- owner_wave/tests.rs
- ContentManifest
- public_systems/tests.rs
- invalid
- Bloxgloom
- process_movement_batch
- drop_merge.rs
- colliders.rs
- arc
- lifecycle-fixture/src/machine.rs
- removal
- mlua
- cold.rs
- EventRealm
- install
- widgets.rs
- script_startup/gameplay/player_inventory.rs
- .first_solid_top
- Registry
- journal/tests.rs
- generation
- coder.md
- validate_spawn_volume
- coder-fast.md
- script_startup/machine/components.rs
- owner_durable/tests.rs
- InventoryWorkers
- EffectConsumerScratch
- .draw_node
- Insects
- decode
- spawn.rs
- GpuPass
- script_startup/machine.rs
- Result
- fields_with_command
- invoke
- .plan
- players/inventory.rs
- .append_batch
- resolve_player_movement
- declarer
- generate.py
- MobilePages
- plan_observed_request
- Behavior
- moving/lifecycle.rs
- prepare_recovery
- State
- run_perf_benchmark_async
- streaming.rs
- render_previews_weather
- integer
- validate
- notifications.rs
- render/effects/tests.rs
- Behavior
- plan
- Public dynamic-entity surface
- Bindings
- coordinates
- gameplay
- client/admin.rs
- render
- Archived plans and audits
- .decode_reader
- ClientApp
- ClientMessage
- Candidate
- UiFrame<'_>
- server/checkpoint/tests.rs
- .validate_player_selection
- join_worker.rs
- ObserverRegistration
- visual/tests.rs
- .register_inventory_screen
- run
- custom/tests.rs
- General anchored block entities
- join_lifecycle.rs
- script_startup/moving.rs
- register
- quad
- CharacterAsset
- script_startup/gameplay/player_operations.rs
- WorldTime
- crate
- Control
- DroppedItem
- coder-smart.md
- Phase 8: examples, parity and integrated verification
- native.rs
- run_loop
- Agent guidance
- startup/moving/tests.rs
- manifest.json
- remove
- .decode
- shader
- resources.rs
- EffectBuffer
- .handle
- public_systems/motion/tests.rs
- recipe_browser.rs
- client/appearance.rs
- simulation
- .player_lifecycles
- reviewer.md
- Presentation
- prepare_recovery
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
- plan_inner
- .prepare_benchmark_frontier_wave
- commands.rs
- Preset
- duration
- declarer
- script_startup/gameplay/inventory.rs
- time.rs
- Resampler
- PlayerDecision
- admin/tests.rs
- gameplay/admin.rs
- src/appearance.rs
- route_effects
- app
- client_metadata.rs
- .rotate_using
- require
- Cache
- Larger Luau packages and independent simulation features
- client/observations/tests.rs
- Context<'_>
- seed
- Player lifecycle implementation
- axis
- PlayerState
- SpawnReceipt
- entities/moving_tests.rs
- presentation/effects/tests.rs
- colliders/tests.rs
- .aimed_mobile
- temp_save_dir
- server/drops.rs
- decode
- Modding: start here
- .encode
- welcome/assets/fonts/FONT.md
- drop_pickup.rs
- visual_contracts.rs
- client/audio/tests.rs
- src/storage/tests.rs
- Chunk
- package
- Registered content and composition
- load.rs
- AppearanceState
- EffectKindId
- SCRIPTING.md
- crouch.rs
- create_target_pipeline
- invalid
- Capture
- declarer
- memory/tests.rs
- system/decisions.rs
- target_actions.rs
- materials.rs
- .generate
- Authoritative moving entities
- declarations/moving/tests.rs
- Slots
- prepare
- decode
- CharacterRenderer
- audio/SCRIPTING.md
- Startup
- render_block_preview
- render_daylight_previews
- inventory
- Articulated characters
- declarations/budget.rs
- record
- Game weather foundation
- sample
- ServerMessage
- presentation/observations/tests.rs
- src/storage.rs
- DropPolicy
- face/README.md
- FireAnimator
- first_person/tests.rs
- outbound/tests.rs
- Reaction
- Appearance
- world_time/tests.rs
- EffectBuffer
- WorkstationView
- Spawn
- journal/recovery.rs
- CharacterEditor
- state
- .from_builtin_parts
- src/daylight.rs
- coordinator/tests.rs
- EffectConsumerOutput
- Luau VM lifetime and module state
- Budget
- tcp/report.rs
- Procedural
- stack
- script_startup/machine/variants.rs
- deviceext
- render_weather_previews
- std
- validate
- durable/fire/tests.rs
- MovementState
- .sound
- world
- probe_request
- content/moving/tests.rs
- exposure
- Extension
- visibility.rs
- Registered inventory views and screens
- .build
- passes
- render/weather/tests.rs
- catalog
- lifecycle-fixture/src/content.rs
- extension_system.rs
- character_asset/gameplay.rs
- prepare
- Articulated renderer performance
- .new
- plan_changes
- Content
- mpsc
- entities/motion.rs
- plan_event
- listener_chain
- visit
- Accepted goal: authoritative moving entities and projectiles
- src/motion/tests.rs
- write_frame
- server/weather/tests.rs
- Atlas
- script/tests.rs
- validate_changes
- validate_spawn
- session_ids/tests.rs
- SharedParts
- prepare
- transfer/tests.rs
- Catalog
- install
- transfer
- engine/tests.rs
- Registered anchored behavior
- Clock
- declarer
- Registered actions and composed controls
- Public storage lifecycle boundary
- Registered inventory machines
- server/movement/teleport.rs
- showcase.rs
- bundle_ui.rs
- script_startup/drop_policy.rs
- .place_or_interact
- tests/effects.rs
- receive_content_manifest
- Client audio foundation
- avatars/moving_tests.rs
- .gameplay_observers
- .owner_systems
- receipts/tests.rs
- Cadence
- present
- modding/README.md
- register
- menus/tests.rs
- vm_latency.rs
- metadata
- State
- EffectLimits
- actors/moving/tests.rs
- .sample_gameplay_look
- fog.rs
- active.rs
- saturating_wake_emitter
- replant.rs
- weather/codec.rs
- Dynamic authored UI and input
- validate_sources
- TEMP_ID

## God Nodes (most connected - your core abstractions)
1. `EntityError` - 235 edges
2. `SystemId` - 141 edges
3. `OwnerKey` - 140 edges
4. `StateKey` - 118 edges
5. `Value` - 111 edges
6. `TickId` - 103 edges
7. `ClientMessage` - 101 edges
8. `ScriptError` - 98 edges
9. `CommitAction` - 91 edges
10. `world_to_chunk()` - 86 edges

## Surprising Connections (you probably didn't know these)
- `Existing capabilities exposed` --references--> `geometry()`  [INFERRED]
  docs/modding/REGISTERED-CONTENT.md → tools/character_assets/articulated.py
- `Textures and blocks` --references--> `geometry()`  [INFERRED]
  SCRIPTING.md → tools/character_assets/articulated.py
- `F4 — New flammability meets a separate edit producer` --references--> `apply_synced_batch()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/server/durable/fire.rs
- `Validate` --references--> `require()`  [INFERRED]
  docs/modding/IDE.md → tools/character_assets/glb.py
- `Libraries` --references--> `require()`  [INFERRED]
  docs/modding/RUNTIME-TOOLS.md → tools/character_assets/glb.py

## Import Cycles
- 2-file cycle: `src/server/script/runtime.rs -> src/server/script/runtime/diagnostics.rs -> src/server/script/runtime.rs`
- 2-file cycle: `src/server/perf/tcp.rs -> src/server/perf/tcp/report.rs -> src/server/perf/tcp.rs`
- 2-file cycle: `src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server/block_actions.rs`
- 2-file cycle: `src/audio/procedural/rain.rs -> src/audio/procedural/rain/scene.rs -> src/audio/procedural/rain.rs`
- 3-file cycle: `src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server/net.rs`
- 3-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server.rs`
- 3-file cycle: `src/server.rs -> src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server.rs`
- 3-file cycle: `src/server/parallel.rs -> src/server/parallel/owner_wave.rs -> src/server/registry.rs -> src/server/parallel.rs`
- 4-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server.rs`

## Communities (831 total, 296 thin omitted)

### Community 0 - "EntityTypeDescriptor"
Cohesion: 0.08
Nodes (5): EntityPayloadCodec, EntityTypeDescriptor, EntityTypeRegistration, EntityTypeRegistryBuilder<'a>, TickPolicy

### Community 1 - "EntityStore"
Cohesion: 0.07
Nodes (40): encode_allocator_value(), encode_motion_value(), encode_cell_owner(), validate_ownership_mode(), allocator_state_key(), apply_operation_to_projection(), canonical_location(), cell_state_key() (+32 more)

### Community 2 - "Port<P>"
Cohesion: 0.16
Nodes (4): Interaction, Port, Port<P>, public_slots()

### Community 3 - "super"
Cohesion: 0.02
Nodes (10): abandoned_result_retires_transport(), completed_result_can_still_be_cancelled(), failure_and_retry_dispatch(), MESHES, progress_rotation(), Codec, mixed_runtime_catalog_joins_restarts_and_remaps_saved_identities_without_execution(), packages() (+2 more)

### Community 4 - "render_previews"
Cohesion: 0.31
Nodes (23): render_avatar_preview(), render_character_motion(), render_character_preview(), render_character_styles(), render_chest_previews(), render_creature_preview(), render_drop_animation_previews(), render_drop_preview() (+15 more)

### Community 5 - "state_for"
Cohesion: 0.10
Nodes (43): mismatched_player_catalog_preserves_existing_world_and_profile_files(), old_profile_formats_are_rejected_without_resetting_or_rewriting_them(), profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default(), profile_appearance_save_failure_never_publishes_and_stops_mutation(), recipe_save_failure_never_publishes_and_invalid_selection_never_writes(), recipe_save_is_profile_bound_canonical_and_exposes_only_legacy_host_projection(), drain_durable(), drop_active_len() (+35 more)

### Community 6 - "world_to_chunk"
Cohesion: 0.13
Nodes (43): bin_catalog(), bin_count(), bin_startup(), BinPayload, BinPullTick, coordinator_drain_preserves_deferred_entity_tick_until_commit(), drive_poke_tick(), entity_and_player_views_merge_with_deterministic_order() (+35 more)

### Community 7 - "VoxelView"
Cohesion: 0.04
Nodes (29): Inventory, CapturedColumn, DropTickPlanner, AnchorEchoTick, BadTick, BinExchange, CounterInteract, CounterTick (+21 more)

### Community 8 - "output.rs"
Cohesion: 0.08
Nodes (17): Controls, apply_latest_controls(), AudioOutput, COMMAND_CAPACITY, fill(), Frame, OUTPUT_BATCH, OutputStats (+9 more)

### Community 9 - "OwnerData"
Cohesion: 0.05
Nodes (17): OwnerCodec, OwnerData, Codec, config(), prepare(), prepare_writes(), OwnerCodecError, OwnerValueCodec (+9 more)

### Community 10 - "server/runtime/tests.rs"
Cohesion: 0.06
Nodes (43): captured_owner_reads_share_reservations_and_fence_exclusive_waves(), rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor(), due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility(), increment(), invalid_handler_deadline_rejects_without_losing_due_work(), mixed_active_and_recurring_due_owners_progress_with_and_without_wakes(), multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation(), run() (+35 more)

### Community 11 - "Complete modding implementation proposal"
Cohesion: 0.05
Nodes (39): 10. Custom models: explicitly deferred, 11. Developer workflow and maintenance rules, 12. Implementation order and deliverables, 13. Completion and verification, 14. Continuation record — update during implementation, 1. What approval means, 2. Product outcome, 3. Starting point: preserve the useful work (+31 more)

### Community 12 - "lifecycle-fixture/src/system.rs"
Cohesion: 0.13
Nodes (12): Clock, definition(), KEY, NeighborProbe, Pair, pair_definition(), Probe, WakeLoop (+4 more)

### Community 13 - "OwnerKey"
Cohesion: 0.05
Nodes (18): ChunkKey, MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_PATCH_BYTES_PER_JOB, MAX_OWNER_PATCH_WRITES_PER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, MAX_OWNER_WAVE_PATCH_WRITES, OwnerJob, OwnerJobError (+10 more)

### Community 14 - "scheduler.rs"
Cohesion: 0.07
Nodes (21): cursor_lane(), FireRuntime, source_transaction(), FIRE_DELIVERY_SYSTEM_ID, FIRE_LANES, FIRE_SYSTEM_ID, FireCursor, FireLoadMetrics (+13 more)

### Community 15 - "server/durable.rs"
Cohesion: 0.05
Nodes (19): stage(), Durability, BlockDelta, CHECKPOINT_QUEUE_CAPACITY, CHECKPOINT_WORKERS, DirtyCheckpoint, Durability, FireCheckpointBatch (+11 more)

### Community 16 - "view"
Cohesion: 0.09
Nodes (18): Mossbun, View, BODY, gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling(), ground_motion_respects_walls_cliffs_seams_and_embedded_edits(), view(), BODY, register() (+10 more)

### Community 17 - "PendingWakeStore"
Cohesion: 0.08
Nodes (25): crc32(), decode_owner_wake_key(), decode_wake_value(), encode_wake_value(), invalid_data(), OWNER_WAKE_DOMAIN, owner_wake_key(), OWNER_WAKE_MAGIC (+17 more)

### Community 18 - "SystemId"
Cohesion: 0.05
Nodes (39): OwnerPartition, SystemId, crc32(), decode_cell_value(), decode_cursor_value(), decode_owner_cursor_key(), decode_owner_state_key(), encode_cell_value() (+31 more)

### Community 19 - "rain.rs"
Cohesion: 0.09
Nodes (25): band_next(), Bed, BED_BANDS, diameter(), drop(), Droplet, DropVoice, MAX_DROPS (+17 more)

### Community 20 - "PackageSnapshot"
Cohesion: 0.05
Nodes (7): authored_drop_animation_negotiates_and_default_keeps_old_bundle(), drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle(), item_sprite_option_is_negotiated_and_omission_preserves_default_identity(), handler_declarer(), error(), Package, PackageSnapshot

### Community 21 - "Error"
Cohesion: 0.07
Nodes (8): Block, cell_random(), Context, Context<'a>, DropSpawn, Error, Plan, Snapshot

### Community 22 - "protocol.rs"
Cohesion: 0.11
Nodes (30): BLOCK_COUNT, Cursor, Cursor<'a>, frame(), invalid(), key(), MAX_ENTITY_INTERACT_BYTES, MAX_FIRE_BURSTS (+22 more)

### Community 23 - "VisualAvatar"
Cohesion: 0.12
Nodes (8): ActorAnimator, DELAY, Track, Sample, STEP, Track, Motion, VisualAvatar

### Community 24 - "OutboundFrame"
Cohesion: 0.10
Nodes (11): ClientQueueTelemetry, OUTBOUND_AGGREGATE_BYTE_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundError, OutboundFrame, OutboundQueue, OutboundTelemetry (+3 more)

### Community 25 - "StateKey"
Cohesion: 0.10
Nodes (23): decode_snapshot(), encode_snapshot(), FILE_NAME, MAGIC, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_KEYS, read_snapshot_file(), TEMP_SEQUENCE (+15 more)

### Community 26 - "WorldSnapshot"
Cohesion: 0.07
Nodes (10): block(), combine_entities(), dispatch_neighbors(), error(), OperationInput, Participants, plan_removals(), plan_with_lifecycles() (+2 more)

### Community 27 - "MobileProbe"
Cohesion: 0.06
Nodes (10): MobileProbe, NetworkedVisualProbe, console_commands_reject_non_admin_and_recover_grant_over_nonblocking_listener(), creature_probe(), external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener(), mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart(), mixed_work(), response_samples() (+2 more)

### Community 28 - "receipts.rs"
Cohesion: 0.11
Nodes (18): Admission, checksum(), invalid(), MAGIC, MAX_PAYLOAD, MAX_REASON, MAX_SNAPSHOT, ReceiptEvent (+10 more)

### Community 29 - "RegistrationError"
Cohesion: 0.39
Nodes (5): Cursor, Cursor<'a>, invalid(), length(), validate_target()

### Community 30 - "handler.rs"
Cohesion: 0.07
Nodes (15): FireFrontier, FireDeliveryHandler, FireDeliveryInput, FireDeliveryPatch, FireHandler, FireOwnerInput, FireOwnerPatch, MAX_DELIVERIES_PER_OWNER (+7 more)

### Community 31 - "Handler"
Cohesion: 0.06
Nodes (19): Event, EventKind, Handler, HandlerRegistration, RemovalCause, ChestBreak, CollectDrop, FlowerNeighbor (+11 more)

### Community 32 - "ui/draw.rs"
Cohesion: 0.10
Nodes (23): UiBuilder<'_>, UiBuilder<'_>, EDGE, FONT_HEIGHT, FONT_WIDTH, GOLD, inset(), item_color() (+15 more)

### Community 33 - "SystemRuntime"
Cohesion: 0.08
Nodes (8): MAX_OWNER_VALUES_PER_SYSTEM, MAX_PENDING_OWNER_WAKES, PendingRegisteredWave, PreparedRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs, StagedOwnerCommit, SystemRuntime

### Community 34 - "server_state_with_startup"
Cohesion: 0.12
Nodes (39): tick_once(), server_state_with_startup(), durable_counter_startup(), durable_pair_startup(), durable_twin_startup(), entity_and_owner_state_commit_as_one_atomic_record(), external_neighbor_reads_defer_until_all_chunks_arrive_and_fence_adjacent_edits(), external_owner_can_durably_wake_another_owner_after_restart() (+31 more)

### Community 35 - "EntityIndexes"
Cohesion: 0.11
Nodes (14): _entity_cell_key_round_trip(), _entity_page_key_round_trip(), Bucket, ChunkPage, decode_cell_key(), decode_cell_owner(), decode_chunk_key(), encode_cell_key() (+6 more)

### Community 36 - ".window_event"
Cohesion: 0.10
Nodes (5): ClientApp, ClientApp, escape_screen(), inventory_screen(), ClientApp

### Community 37 - "Error"
Cohesion: 0.08
Nodes (9): cached_profile_inventory_rechecks_each_handler_authority_and_latches_denial(), handler_random_is_stable_per_seed_cell_and_registration(), ignored_failures_cannot_publish_partial_operations(), Inventories, moving_spawn_references_are_local_and_never_predict_durable_ids(), private_entity_overlay_does_not_authorize_the_next_decision_owner(), staged_motion_rechecks_owner_coalesces_fields_and_rejects_caught_errors(), transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides() (+1 more)

### Community 38 - "intent.rs"
Cohesion: 0.09
Nodes (19): blocked(), decode(), decode_key(), encode(), is_key(), key(), Reader, Reader<'a> (+11 more)

### Community 40 - "journal/rotation.rs"
Cohesion: 0.15
Nodes (37): crc32(), invalid_data(), Base, BASE_FORMAT_VERSION, BASE_FORMAT_VERSION_LEGACY, BASE_MAGIC, BASE_MAX_BYTES, base_path() (+29 more)

### Community 41 - "JobKey"
Cohesion: 0.12
Nodes (6): BarrierError, BatchId, CancelError, JobKey, PhaseExecutor<R, E>, SubmitError

### Community 42 - "public/tests.rs"
Cohesion: 0.12
Nodes (15): BlockExtension, component_schema_package_and_tag_changes_are_compatibility_failures(), Contribute, Declare, Emitter, external_narrow_plant_uses_registered_selection_in_production_raycast(), fixture(), invalid_composition_and_missing_content_fail_atomically_before_installation() (+7 more)

### Community 43 - "BlockActionContext"
Cohesion: 0.06
Nodes (16): BlockActionContext, BlockActionHooks, BlockActionRegistry, BlockActionRegistryBuilder, BlockActionRegistryBuilder<'a>, BlockCommitBuilder, MAX_BLOCK_ACTION_HANDLERS, probe_deferred() (+8 more)

### Community 44 - "drops/planning.rs"
Cohesion: 0.13
Nodes (25): merge_target(), plan_error(), plan_expired(), plan_spawn_stack(), plan_spawns(), plan_spawns_with_extra(), plan_stack_spawns(), plan_stack_spawns_with_extra() (+17 more)

### Community 45 - "MobileEntity"
Cohesion: 0.09
Nodes (16): Animation, Behavior, Body, Context, Cuboid, Error, Lifecycle, MobileEntity (+8 more)

### Community 46 - "Renderer"
Cohesion: 0.05
Nodes (6): next_upload_index(), order_pending_mesh(), Renderer, RendererError, RenderStats, urgent_mesh_reorders_existing_pending_chunk_without_duplication()

### Community 47 - "UiLayout"
Cohesion: 0.10
Nodes (10): InventorySearch, search_rect(), join_action_rect(), centered_panel(), HitRect, UiLayout, SettingId, UiControl (+2 more)

### Community 48 - "server/fire/tests.rs"
Cohesion: 0.12
Nodes (32): cursor_key(), frontier_key(), benchmark_frontier_bootstrap_precedes_first_live_fire_tick(), checkpoint_batch_replaces_one_complete_snapshot_and_applies_tombstones(), checkpoint_store_rejects_orphans_and_corruption_and_cleans_interrupted_temp(), chunk(), durable_lane_age_prioritizes_an_owner_deferred_by_wal_pressure(), first_aggregate_write_preserves_legacy_per_key_checkpoints() (+24 more)

### Community 49 - "StorageBlockEntity"
Cohesion: 0.18
Nodes (10): MAX_FOOTPRINT, MAX_STORAGE_SLOTS, PlacementContext, PlaceStorage, RemovalContext, RemoveStorage, StorageBlockEntity, storage() (+2 more)

### Community 50 - "server/effects.rs"
Cohesion: 0.17
Nodes (12): block_change_owners(), CellCoord, Effect, EffectBatch, MAX_EFFECTS_PER_BATCH, MAX_EFFECTS_PER_OWNER, MAX_EFFECTS_PER_PRODUCER_TICK, owner_order() (+4 more)

### Community 51 - "protocol/tests.rs"
Cohesion: 0.11
Nodes (31): read_server(), action_receipts_round_trip_and_reject_invalid_ids(), admin_grant_wire_round_trips_and_rejects_invalid_counts(), catalog_with_many_states(), character_selection_is_session_scoped_and_bounded_on_wire(), client_messages_round_trip(), committed_action_spawn_mappings_roundtrip_and_reject_invalid_ordinals(), committed_fire_cues_round_trip_with_a_strict_cell_bound() (+23 more)

### Community 52 - "bench.rs"
Cohesion: 0.07
Nodes (18): FireApplyTimings, FireRuntime, ACTIVE_CHUNKS, benchmark_cpu(), BenchSave, cell_index(), CHUNKS_X, CHUNKS_Z (+10 more)

### Community 53 - "intent/tests.rs"
Cohesion: 0.13
Nodes (29): durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans(), durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation(), durable_intent_bootstrap_destination_conflict_retries_one_atomic_record(), durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required(), durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems(), cell(), chunk(), durable_intent_bootstrap_chain_gates_creation_retry_forwarding_and_restart() (+21 more)

### Community 54 - "src/client.rs"
Cohesion: 0.08
Nodes (15): action_id(), ActionTracker, command_action_id(), edit_for_hit(), edit_for_hit_with_catalog(), FRAME, INCOMING_FRAME_BUDGET, Keys (+7 more)

### Community 55 - "Replicas"
Cohesion: 0.12
Nodes (12): Assembly, MAX_CHUNK_ENTITY_BYTES, MAX_CLIENT_ENTITY_BYTES, MAX_PENDING_BYTES, MAX_PENDING_COMMITS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot (+4 more)

### Community 56 - "Item"
Cohesion: 0.11
Nodes (18): Block, BlockState, Components, DropAnimation, .BYTE_LEN, DropSize, FaceTextures, Geometry (+10 more)

### Community 57 - "ServerStartup"
Cohesion: 0.11
Nodes (4): entity_error(), ServerStartup, StartupEntityType, StartupOwnerCodec

### Community 58 - "script.rs"
Cohesion: 0.09
Nodes (11): Invocation, Limits, Output, Program, Request, run(), run_with(), ScriptFailure (+3 more)

### Community 59 - "Registrar"
Cohesion: 0.13
Nodes (3): CubeBlock, Registrar, RegistrationError

### Community 60 - "client/entities/tests.rs"
Cohesion: 0.10
Nodes (31): anchored_presentation_window_is_separate_scoped_and_sorted(), mossbun_adapter_uses_the_negotiated_catalog_assignment(), mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction(), presentation_entity_window_is_ordered_and_explicitly_bounded(), accept(), block_and_entity_changes_wait_for_whole_cross_chunk_commit(), checksum_conflict_and_revision_gap_request_resync_without_partial_install(), chunk() (+23 more)

### Community 61 - "server/drops/tests.rs"
Cohesion: 0.11
Nodes (30): active_len(), apply_expired(), assert_store_consistent(), drop_world(), drop_world_in(), DropWorld, insert_entry(), item() (+22 more)

### Community 62 - "parallel.rs"
Cohesion: 0.12
Nodes (20): CancellationToken, execute_task(), ExecutorConfigError, JobCompletion, JobOutcome, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS (+12 more)

### Community 63 - "perf/fixture.rs"
Cohesion: 0.07
Nodes (33): ACTION_INTERVAL, add_clients_and_seed_drops(), DIRT_ITEM, drain_outbound(), DrainTotals, DROP_HEIGHTS, install_chunks(), MAX_STEADY_TICKS (+25 more)

### Community 64 - "World"
Cohesion: 0.10
Nodes (6): ChunkKey, ChunkReadStamp, EditBasis, LoadedChunk, PreparedEdit, World

### Community 65 - "OwnerEffectPatch"
Cohesion: 0.09
Nodes (7): BlockEdit, EditCause, blocked(), EmittedOwnerEffect, OwnerEffectPatch, route_and_consume(), RoutedOwnerWakes

### Community 67 - "actions/entity.rs"
Cohesion: 0.18
Nodes (18): capture_dependencies(), capture_entity_view_for_plan(), capture_tick_input(), capture_view_for_plan(), commit_tick_plan(), corrupt(), interaction_sight(), permission() (+10 more)

### Community 68 - "PalettedBlocks"
Cohesion: 0.10
Nodes (6): LocalIndex, PalettedBlocks, PaletteView, set_palette_cell(), u16, u8

### Community 69 - "server/entities/tests.rs"
Cohesion: 0.13
Nodes (28): decode_checkpoint(), encode_checkpoint(), a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema(), anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically(), checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types(), delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion(), DROP_TYPE, fake_neighbour() (+20 more)

### Community 70 - "conflict_tests.rs"
Cohesion: 0.12
Nodes (24): action(), disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply(), drop_merge_absence_is_fenced_against_same_owner_motion_into_range(), hold(), neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders(), overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership(), plan(), PROBE (+16 more)

### Community 71 - "Contributor"
Cohesion: 0.10
Nodes (11): CHUNK_SIZE, Context, Contributor, in_world_bounds(), MAX_WRITES, mix(), Registration, SampleError (+3 more)

### Community 72 - "protocol/sounds.rs"
Cohesion: 0.09
Nodes (17): controls(), Event, identifier(), key(), Kind, position(), State, Voice (+9 more)

### Community 73 - "perf/fire.rs"
Cohesion: 0.11
Nodes (22): ACTIVE_CHUNKS, CHUNKS_X, CHUNKS_Z, drain_durable(), ensure_resident(), fixture_action(), FOREST_BATCH_CHUNKS, forest_hash() (+14 more)

### Community 74 - "ChunkCache"
Cohesion: 0.12
Nodes (3): CacheEntry, ChunkCache, OwnerState

### Community 76 - "PublicEntity"
Cohesion: 0.14
Nodes (26): BlockCellChange, enforce_frame_size(), EntitySnapshotPage, MAX_BLOCK_CHANGES_PER_PART, MAX_ENTITIES_PER_PAGE, MAX_ENTITY_CHANGES_PER_PART, MAX_ENTITY_SNAPSHOT_BYTES, MAX_ENTITY_SNAPSHOT_PAGES (+18 more)

### Community 78 - "key"
Cohesion: 0.28
Nodes (14): animated_item_bundle_rejects_invalid_and_noncanonical_motion(), canonical_order_dependency_identity_and_count_bounds_are_verified(), decoder_rejects_server_classification_and_oversized_payloads_before_copying(), header(), key(), material_effect_and_ui_assets_coexist_in_canonical_bundle(), metadata_validates_namespace_capability_shape_and_limits_before_compilation(), package() (+6 more)

### Community 79 - "reactor.rs"
Cohesion: 0.13
Nodes (16): ACCEPT_BUDGET, has_admission_capacity(), has_admission_capacity_with_limit(), INVENTORY_WORKERS, IO_POLL_TIMEOUT, LISTENER_KEY, MAX_PENDING_LEAVES, READ_BUDGET (+8 more)

### Community 80 - "content"
Cohesion: 0.05
Nodes (19): register(), kiln_block_states(), kiln_footprint(), kiln_payload(), kiln_state(), KilnHalf, register_entity_type(), register_entity_type_with_recipes() (+11 more)

### Community 82 - "src/world.rs"
Cohesion: 0.09
Nodes (31): AIR, BEDROCK_Y, BLUE_FLOWER, CHUNK_SIZE, CHUNK_VOLUME, DIRT, FERN, GLOWSTONE (+23 more)

### Community 83 - "InventoryStore"
Cohesion: 0.13
Nodes (11): checksum(), HEADER_LEN, invalid(), InventoryStore, MAGIC, MAX_LEN, MIN_LEN, SLOT_FIXED_LEN (+3 more)

### Community 84 - "Change"
Cohesion: 0.06
Nodes (22): Change, arbitrate_key_sets(), build_owner_writes_parallel(), canonical_key_set(), OwnerCommit, OwnerWaveDurables, OwnerWorldAction, canonical_key_sets_collapse_duplicates_and_sort() (+14 more)

### Community 85 - "TickSample"
Cohesion: 0.11
Nodes (14): duration_nanos(), EVENT_LATENCY_STREAMS, LatencyEvent, LatencyRing, Metric, MetricsRecorder, MotionSample, nearest_rank() (+6 more)

### Community 86 - "tcp.rs"
Cohesion: 0.11
Nodes (15): drive(), exercise(), movement(), open_nuisance_peers(), PROFILE_BASE, run(), SEED, seed_inventories() (+7 more)

### Community 87 - "Declarations"
Cohesion: 0.09
Nodes (6): declaration_key(), Declarations, invoke(), PackageTexture, Pending, text()

### Community 88 - "GameUi"
Cohesion: 0.09
Nodes (5): draw_screen(), DrawTarget, GameUi, Intent, themed_context()

### Community 89 - "Diagnostics"
Cohesion: 0.13
Nodes (12): Buffer, Diagnostics, encode_fields(), full(), invalid(), MAX_BYTES, MAX_MESSAGE, MAX_RECORDS (+4 more)

### Community 90 - "owner/tests.rs"
Cohesion: 0.11
Nodes (19): Behavior, CELL, drop_count(), empty_action(), Fixture, Harvest, KEY, Marker (+11 more)

### Community 91 - "Effect"
Cohesion: 0.12
Nodes (3): Effect, target_sizes(), targets_remain_bounded_at_large_and_tiny_viewports()

### Community 92 - "JournalWriter"
Cohesion: 0.09
Nodes (7): CommitReceipt, Journal, JournalWriter, Request, RotateError, RotationReceipt, WriterCommand

### Community 93 - "Authored materials and effects"
Cohesion: 0.40
Nodes (5): Authored materials and effects, Effect contract 2, Material contract 2, Preparation and compatibility, Typed values from Luau

### Community 94 - "CheckpointWriter"
Cohesion: 0.10
Nodes (7): checkpoint_shard(), checkpoint_worker(), CheckpointJob, CheckpointReceipt, CheckpointSubmitError, CheckpointWriter, panic_message()

### Community 95 - "RegisteredEffectIntent"
Cohesion: 0.18
Nodes (5): ErasedEffectKind, RegisteredEffectBuffer<'a>, RegisteredEffectError, RegisteredEffectIntent, TypedEffectKind<P, M, V, D, C>

### Community 96 - "EntityCheckpointMirror"
Cohesion: 0.10
Nodes (10): CheckpointReceipt, CheckpointTicket, CheckpointWork, Command, EntityCheckpointMirror, Event, MAX_MIRROR_ADMISSIONS, MirrorMetrics (+2 more)

### Community 97 - "Connection"
Cohesion: 0.11
Nodes (8): JoinResponse, Connection, PendingWrite, PendingWriteKind, Phase, PendingLeave, JoinGuard, SimulationInput

### Community 98 - "ClientHandle"
Cohesion: 0.14
Nodes (9): ActionKind, active_slow_peer(), ClientHandle, ClientStats, handshake(), PendingAction, read_until_stop(), Ready (+1 more)

### Community 99 - "TickId"
Cohesion: 0.10
Nodes (20): apply_synced_batch(), Durability, run_delivery(), run_source(), stage_gameplay_burn(), stage_transactions(), stage_wave(), ClockError (+12 more)

### Community 100 - "Value"
Cohesion: 0.13
Nodes (19): Value, bytes(), decode(), delay(), component_schema(), drop_policy(), options(), hexadecimal_schema_fingerprint_is_exact_and_cannot_mix_encodings() (+11 more)

### Community 101 - "CommitAction"
Cohesion: 0.08
Nodes (32): CommitAction, batchable_motion(), cancel_prepared_entities(), command_action_id(), defer_action(), durable_request_profile(), fail_if_durability_failed(), fatal_stage_error() (+24 more)

### Community 102 - "third_person.rs"
Cohesion: 0.11
Nodes (11): avatar(), GameplayPose, prepare(), render_first_person_previews(), render_gameplay_animation_previews(), render_third_person_previews(), Shot, DISTANCE (+3 more)

### Community 103 - "package/client.rs"
Cohesion: 0.05
Nodes (35): ANIMATED_MAGIC, APPEARANCE_MAGIC, APPEARANCE_POLICY_MAGIC, BLOCK_OPTIONS_MAGIC, BLOCK_STATES_MAGIC, CacheKey, ClientBundle, ClientPackage (+27 more)

### Community 104 - "voxel_view.rs"
Cohesion: 0.12
Nodes (12): air_chunk(), key(), MissingChunk, MovementError, player_collides(), resolve_player_movement(), resolve_player_movement_with_body(), set_block() (+4 more)

### Community 106 - "durable/state.rs"
Cohesion: 0.18
Nodes (11): stage(), touches_anchor(), action_changes(), chunk_state_key(), decode_chunk_key(), decode_profile_key(), encode_action_receipt(), encode_action_receipt_with_catalog() (+3 more)

### Community 107 - "avatars.rs"
Cohesion: 0.05
Nodes (16): AvatarInstance, AvatarModel, AvatarRenderer, CharacterInstance, GROUPS, JOINTS, MATERIALS, character_shader() (+8 more)

### Community 108 - "dispatch.rs"
Cohesion: 0.14
Nodes (15): apply(), disconnect(), prepare(), Prepared, publish(), effect(), fire_cue_is_dropped_for_backlogged_client_without_disconnect(), fire_cue_requires_subscription_and_follows_committed_world_frame() (+7 more)

### Community 109 - "menus.rs"
Cohesion: 0.30
Nodes (12): actions(), admin(), button(), draw(), EDGE, GOLD, MUTED, PANEL (+4 more)

### Community 110 - "solver.rs"
Cohesion: 0.15
Nodes (45): Blocked, Body, cap_speed(), check_bounds(), Collider, Contact, ContactMemory, ContactPolicy (+37 more)

### Community 111 - "terrain.rs"
Cohesion: 0.17
Nodes (28): Biome, collapse_surface(), Column, decorate_chunk(), generate_blocks(), generated_block(), generated_block_in_column(), generated_block_with_pattern() (+20 more)

### Community 112 - "Atmosphere"
Cohesion: 0.12
Nodes (6): Atmosphere, smooth(), create_sky_pipeline(), sky_camera_data(), SKY_SHADER, sky_basis_tracks_camera_turns_in_world_space()

### Community 113 - "ComponentMatch"
Cohesion: 0.15
Nodes (13): ComponentMatch, ComponentOutput, ComponentValue, value_bytes(), decode_exact(), decode_input(), decode_output(), encode_exact() (+5 more)

### Community 114 - "ChunkLoader"
Cohesion: 0.11
Nodes (9): ChunkLoader, ChunkLoadResult, ChunkLoadTicket, Job, RequestError, RequestStatus, stop_workers(), WORKER_COUNT (+1 more)

### Community 115 - "Journal"
Cohesion: 0.08
Nodes (5): frame_len(), Journal, KnownRecord, SubmitError, Transaction

### Community 116 - "script_startup/system.rs"
Cohesion: 0.15
Nodes (21): commit(), Fixture, KEY, luau_burn_owner_uses_host_removal_semantics_and_persists_receipt(), luau_owner_after_dependencies_are_resolved_before_save_creation(), luau_owner_block_info_uses_captured_public_fields_and_restarts(), luau_owner_caught_invalid_entity_change_rejects_whole_wave(), luau_owner_drop_creation_shares_receipt_and_restarts() (+13 more)

### Community 118 - "world/tests.rs"
Cohesion: 0.10
Nodes (26): biomes_cover_distinct_surfaces_across_an_endless_world(), broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline(), chunk_cache_evicts_the_least_recently_used_resident(), collapsed_surface_obeys_constraints_and_matches_region_edges(), composed_generation_is_ordered_and_does_not_change_builtin_baseline(), composed_generation_rejects_unknown_states_and_duplicate_keys(), corrupt_save_is_not_silently_discarded(), edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot() (+18 more)

### Community 119 - "Clock"
Cohesion: 0.13
Nodes (4): advance(), Clock, DOMAIN, publish()

### Community 120 - "PlayerRules"
Cohesion: 0.11
Nodes (9): Body, BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_RULES, BUILTIN_SPAWN, InvalidPlayerRules, MotionRates, PlayerRules (+1 more)

### Community 121 - "entities/checkpoint.rs"
Cohesion: 0.13
Nodes (13): CHECKPOINT_NAME, checkpoint_rejects_bad_magic_version_and_checksum(), checkpoint_write_read_is_atomic_and_bounded(), crash_left_temporary_checkpoint_fails_closed(), DIRECTORY_NAME, entity_invalid_data(), EntityCheckpointStore, invalid_data() (+5 more)

### Community 122 - "serve"
Cohesion: 0.05
Nodes (42): luau_action_block_targets_keep_real_reach_sight_and_identity_checks(), luau_creature_replaces_itself_with_another_authored_type_over_real_listener(), mod_admin_grant_requires_server_identity_and_replays_once_over_listener(), mod_admin_spawn_and_drop_share_one_allocator_and_restart(), connect(), package(), package_cube_flags_are_frozen_and_old_declaration_keeps_defaults(), package_cube_joins_places_and_recovers_with_identical_session_catalog() (+34 more)

### Community 123 - "burn.rs"
Cohesion: 0.11
Nodes (11): assert_burned(), assert_uncommitted(), Burn, burn_startup(), BurnGrass, definition(), destination(), plant_burn() (+3 more)

### Community 126 - "HarvestSnapshot"
Cohesion: 0.09
Nodes (12): only_block_items_are_placeable(), placeable_block(), placeable_block_in(), SAPLING, SEEDS, STICK, valid_item(), valid_item_in() (+4 more)

### Community 127 - "snapshots/tests.rs"
Cohesion: 0.15
Nodes (17): captured_revision_is_rejected_after_confirmed_edit_and_recaptured_in_order(), changed_interest_or_reconnected_session_cannot_receive_pending_capture(), dense_snapshot_disconnects_only_affected_client_and_closes_earlier_jobs(), differing_epochs_do_not_share_wire_content(), distinct_chunk_capture_is_bounded_and_rotates_to_deferred_clients(), Fixture, live_stream_prepares_once_and_shares_encoded_pages_for_matching_clients(), pressure_reclaim_is_worker_prepared_and_coordinator_announces_then_releases() (+9 more)

### Community 128 - "authored/tests.rs"
Cohesion: 0.06
Nodes (40): combined_mod_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS, farming_scale_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS (+32 more)

### Community 129 - "ScriptCreature"
Cohesion: 0.12
Nodes (4): invalid(), ScriptCreature, State, tick_helpers_keep_order_and_exact_intervals_above_float_precision()

### Community 130 - "script/runtime.rs"
Cohesion: 0.13
Nodes (10): begin(), create(), reseed(), Seed, author_reseeding_keeps_standard_math_random_semantics(), diagnostic_encoding_rejects_nested_fields_without_executing_metamethods(), failed_attempts_keep_diagnostics_and_helper_source_identity(), logging_pressure_does_not_change_random_results_and_retries_repeat_attempts() (+2 more)

### Community 131 - "net.rs"
Cohesion: 0.10
Nodes (16): BUNDLE_TIMEOUT, ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT, JoinCleanup, bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline(), commands_received_after_content_ready_wait_for_join_completion(), connected_streams() (+8 more)

### Community 132 - "ScriptSystem"
Cohesion: 0.11
Nodes (5): bytes(), declarer(), field(), ScriptSystem, table()

### Community 133 - "Plan: built-in/mod capability parity"
Cohesion: 0.08
Nodes (24): 1. Establish the boundary and parity inventory, 2. Complete the container/block-entity vertical slice, 3. Complete dynamic entities and presentation, 4. Close remaining gameplay and world surfaces, 5. Prove integration outside engine internals, Adopted direction, Client presentation and resources, Completed task: player-response path hardening (+16 more)

### Community 134 - "Behavior"
Cohesion: 0.17
Nodes (5): Behavior, Cell, Context, interaction_request(), RemovalCause

### Community 135 - "InventoryId"
Cohesion: 0.20
Nodes (7): Components, Context<'_>, InventoryId, PickupTransfer, PickupTransfer<'a>, Slot, Stack

### Community 136 - "VisualSession"
Cohesion: 0.12
Nodes (3): Script, PendingBatch, VisualSession

### Community 137 - "entity_recovery/tests.rs"
Cohesion: 0.16
Nodes (12): checkpointed_motion_ahead_of_wal_fence_survives_recovery(), fixture(), lagging_checkpoint_replays_later_wal_transfer(), MOBILE_TYPE, NEXT_DIR, one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt(), position(), same_revision_conflicting_checkpoint_motion_fails_closed() (+4 more)

### Community 138 - "effects/registered.rs"
Cohesion: 0.12
Nodes (14): MAX_EFFECT_BATCH_PAYLOAD_BYTES, MAX_EFFECT_BUFFER_PAYLOAD_BYTES, MAX_EFFECT_DESTINATIONS, MAX_EFFECT_KIND_ID_BYTES, MAX_EFFECT_KIND_PAYLOAD_BYTES, MAX_REGISTERED_EFFECT_KINDS, RegisteredEffectBuffer, RegisteredEffectLimits (+6 more)

### Community 139 - "host-api/src/actions.rs"
Cohesion: 0.14
Nodes (15): Action, MAX_ACTIONS, MAX_INTERACTION_PAYLOAD, MAX_REQUEST_ARGUMENTS, MAX_TARGET_ACTIONS, MAX_WIDGETS, Operation, Panel (+7 more)

### Community 140 - "startup/tests.rs"
Cohesion: 0.15
Nodes (19): block_auto_items_cannot_bypass_total_item_capacity(), caught_startup_execution_limit_cannot_publish_and_worker_recovers(), content_capacity_admits_full_block_item_texture_targets(), content_capacity_fixture(), content_capacity_max_plus_one_errors_survive_pcall_with_key_and_usage(), duplicate_keys_and_caught_declaration_errors_reject_all_startup(), Fixture, generator_installation_fixture() (+11 more)

### Community 141 - "Rng"
Cohesion: 0.06
Nodes (11): Biquad, Mode, Rng, SAMPLE_RATE, Cicada, Cicadas, Cricket, Crickets (+3 more)

### Community 142 - "presentation.rs"
Cohesion: 0.15
Nodes (15): bounded_float(), Command, command_entity(), display_text(), EntityView, invalid(), optional_bounded_float(), Reply (+7 more)

### Community 143 - "render/mesh.rs"
Cohesion: 0.15
Nodes (11): LightField, ChunkMesh, emit_plant(), emit_quad(), GpuMesh, GpuSubmesh, mesh_chunk_lit(), mesh_chunk_lit_with_catalog() (+3 more)

### Community 144 - "sync"
Cohesion: 0.16
Nodes (4): Buffer, capture(), default_filter_preserves_structured_game_events_and_flushes_final_errors(), scoped_filters_and_invalid_filter_fallback_work_without_global_state()

### Community 146 - "EntityAdapter"
Cohesion: 0.09
Nodes (7): Capability matrix, Definitions, assets, and composition, Interaction, presentation and durability closure, kiln_adapter(), EntityAdapter, block(), Catalog

### Community 147 - "drops/entity.rs"
Cohesion: 0.12
Nodes (7): DROP_ENTITY_TYPE, DROP_PAYLOAD_FIXED_BYTES, DropEntityPayload, DropPayloadCodec, MAX_DROP_ENTITY_PAYLOAD_BYTES, register_entity_type(), drop_entity_registration_is_catalog_linked_and_mobile()

### Community 148 - "MovingSpawn"
Cohesion: 0.22
Nodes (6): apply(), Context<'_>, MotionChange, MotionCommand, MovingSpawn, SpawnReference

### Community 149 - "world/generation.rs"
Cohesion: 0.14
Nodes (12): apply(), Builtin, BUILTIN_SAMPLES, builtin_state_key(), BuiltinSamples, compose(), generate_chunk(), generate_chunk_with_contributors() (+4 more)

### Community 150 - "publication/commit.rs"
Cohesion: 0.24
Nodes (10): add_remove(), add_upsert(), collect(), CommitChanges, CommitPlan, fanout_bound(), for_client(), KeyChanges (+2 more)

### Community 151 - "entities/types.rs"
Cohesion: 0.06
Nodes (19): validate_location_owner(), AnchorUpdate, CellCoord, EntityId, EntityLocation, EntityOwner, EntityOwnership, EntityPublicView (+11 more)

### Community 152 - "StorageOwner"
Cohesion: 0.21
Nodes (3): StorageNeighbor, StorageOwner, StorageRemoved

### Community 153 - "io"
Cohesion: 0.07
Nodes (19): TURN_ENTRIES, WRITE_BYTES, MAX_FRONTIER_CELLS, CLOCK_DOMAIN, clock_key(), FILE_HEADER_LEN, FILE_MAGIC, FILE_VERSION (+11 more)

### Community 154 - "InventoryScreen"
Cohesion: 0.09
Nodes (7): InventoryScreen, MAX_SLOTS, MAX_STATUS_FIELDS, SlotGroup, StatusField, StatusFormat, Targets

### Community 155 - "render/drops.rs"
Cohesion: 0.15
Nodes (17): block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting(), DropMeshes, emit_cutout_drop(), flower_pickup_uses_cutout_crosses_instead_of_cube_faces(), grass_side_band_is_at_the_top_on_both_side_axes(), is_sprite_item(), MAX_CUTOUT_INDEX_BYTES, MAX_CUTOUT_VERTEX_BYTES (+9 more)

### Community 156 - "Texture"
Cohesion: 0.18
Nodes (5): Texture, Catalog, error(), texture_definition(), validate_display()

### Community 157 - "server.rs"
Cohesion: 0.05
Nodes (36): catalog_with_extension(), Client, DEFAULT_CLIENTS, DEFAULT_VIEW, EDIT_REACH, handle_message(), INPUT_CAPACITY, join_client() (+28 more)

### Community 158 - "custom/shader.rs"
Cohesion: 0.08
Nodes (11): validate(), compose(), STUBS, TYPES, validate(), validate(), compose(), STUBS (+3 more)

### Community 159 - "InventoryProbe"
Cohesion: 0.08
Nodes (4): InventoryProbe, external_item_action_composed_control_receipt_duplicate_stale_and_restart(), send(), send_with_session()

### Community 161 - "Decision"
Cohesion: 0.13
Nodes (6): Behavior, Decision, Event, EventKind, Registration, State

### Community 162 - "Receiver"
Cohesion: 0.07
Nodes (12): Receiver, CodecWorkers, decode_worker(), DECODE_WORKERS, DecodeRequest, encode_worker(), ENCODE_WORKERS, EncodedFrame (+4 more)

### Community 163 - "script_startup/creature.rs"
Cohesion: 0.14
Nodes (12): Flat, luau_creature_interaction_and_animation_negotiate_and_keep_private_state(), luau_creature_negotiates_model_ticks_and_restarts(), luau_creature_neighbour_policy_negotiates_and_reads_bounded_public_views(), luau_creature_options_reject_invalid_bounds_before_save_creation(), luau_creature_rejects_invalid_declaration_before_save_and_caught_route_failure(), luau_creature_rejects_invalid_lifecycle_before_movement_or_state_change(), luau_creature_spawns_another_declared_type_with_its_own_initial_state() (+4 more)

### Community 164 - "Growth foundation plan"
Cohesion: 0.10
Nodes (20): 1. Widen identities and introduce block states as one vertical format transition, 2. Deliver the missing parallel gameplay runtime, 3. Complete generic entity lifecycle and prove anchored behavior with gameplay, 4. Scale the real multiplayer path beyond the former 16-client ceiling, 5. Acceptance gate before calling this foundation complete, Baseline before this campaign (historical), Binding technical requirements, Decisions and invariants (+12 more)

### Community 165 - "world/generation/tests.rs"
Cohesion: 0.16
Nodes (10): absolute_anchor_feature_is_not_truncated_at_chunk_boundary(), authoritative_generation_edit_baselines_survive_cache_miss_and_restart(), builtin_contributor_preserves_terrain_vegetation_and_negative_chunk_seams(), builtin_contributor_preserves_tree_canopy_across_chunk_seam(), files(), generation_failure_never_installs_air_or_replaces_recovery_snapshot(), generation_identity_mismatch_rejects_before_any_save_mutation(), open() (+2 more)

### Community 166 - "extension_lifecycle.rs"
Cohesion: 0.25
Nodes (9): Blocks, inventories, and processing, external_anchored_initialization_use_refund_and_restart_over_real_listener(), external_owner_world_read_survives_real_listener_join_and_restart(), external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp(), external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener(), registered_item_components_transfer_and_recover_over_real_listener(), send(), storage_roundtrip() (+1 more)

### Community 167 - "entity_checkpoint/tests.rs"
Cohesion: 0.18
Nodes (13): admitted_event_permit_cannot_be_dropped_silently(), checkpoint_io_failure_closes_admission_and_reports_error(), delayed_payload_receipt_keeps_newer_checkpoint_only_motion(), fixture(), interrupted_stream_does_not_publish_and_worker_failure_releases_credit(), malformed_ordered_event_fails_closed_without_checkpoint_publication(), multi_turn_checkpoint_holds_generation_fence_without_a_live_capture(), NEXT_TEST_DIR (+5 more)

### Community 168 - "render/effects.rs"
Cohesion: 0.18
Nodes (10): Descriptor, MAX_DESCRIPTOR_BYTES, MAX_PASSES, MAX_SHADER_BYTES, Pass, prepare(), prepare_inner(), Prepared (+2 more)

### Community 169 - "server/movement.rs"
Cohesion: 0.11
Nodes (14): Entities, player behavior, world simulation and generation, AckKind, advance_players(), credit_per_tick(), CREDIT_SCALE, FLOAT_ROUNDING_ALLOWANCE_PER_TICK, MAX_COMMANDS_PER_TICK, max_credit() (+6 more)

### Community 170 - "view.rs"
Cohesion: 0.17
Nodes (14): SlotFilter, draw(), footer(), GOLD, header(), inventory(), machine(), MUTED (+6 more)

### Community 171 - "drops/queries.rs"
Cohesion: 0.20
Nodes (17): age_ms_now(), airborne_count(), capture_nearby(), collect_in_aabb(), distance_sq(), extractable(), has_expired(), live_drop() (+9 more)

### Community 172 - "PlayerEntityStore"
Cohesion: 0.18
Nodes (3): PlayerEntityStore, PlayerPayloadCodec, session_move_and_disconnect_update_public_chunk_ownership()

### Community 173 - "Adapter"
Cohesion: 0.12
Nodes (6): Recipe, Adapter, component_matches(), Lookups, MachinePayload, register()

### Community 174 - "gameplay/decisions.rs"
Cohesion: 0.12
Nodes (12): BLOCK_REGISTER, BLOCK_SOURCE, ByteState, ENTITY_REGISTER, ENTITY_SOURCE, entity_target_action_is_discovered_in_verified_session_catalog(), Fixture, luau_decisions_block_events_caught_error_rollback_and_restart() (+4 more)

### Community 175 - "entities/container.rs"
Cohesion: 0.27
Nodes (3): Codec, ContainerPayload, register()

### Community 176 - "handles.rs"
Cohesion: 0.15
Nodes (14): entity(), entity_value(), EntityId, identity_methods(), intern(), profile(), profile_value(), ProfileId (+6 more)

### Community 177 - "Execution"
Cohesion: 0.12
Nodes (9): Engine, isolated(), MAX_RESIDENT, NEXT_RUNTIME, Reservation, RESIDENT, Retained, RUNTIMES (+1 more)

### Community 178 - "systems/world.rs"
Cohesion: 0.21
Nodes (5): capture(), capture_entities(), EditInputs, plan_edits(), within_radius()

### Community 180 - "custom.rs"
Cohesion: 0.17
Nodes (12): compose(), Descriptor, Material, MaterialSource, MAX_DESCRIPTOR_BYTES, MAX_MATERIALS, MAX_SHADER_BYTES, prepare() (+4 more)

### Community 181 - "init"
Cohesion: 0.14
Nodes (3): DEFAULT_FILTER, filter(), init()

### Community 182 - "Scripting capabilities for mod developers"
Cohesion: 0.07
Nodes (29): module(), Actions, commands and gameplay decisions, Authored materials, effects and Luau parameters, Authoring tools and runnable examples, Client startup and authored UI, Committed server observations, Durable owner systems, Features requiring engine work or native extensions (+21 more)

### Community 183 - "Composition"
Cohesion: 0.13
Nodes (9): Catalog, Composition, field(), MAX_MEMBERS, MAX_PACKAGES, MAX_TAGS, Package, tag_kind() (+1 more)

### Community 184 - "Stack"
Cohesion: 0.15
Nodes (6): ComponentPayload, HOTBAR_SLOTS, MAX_COMPONENT_BYTES, SLOTS, Stack, STACK_LIMIT

### Community 185 - "FirePending"
Cohesion: 0.12
Nodes (12): checked_body(), checksum(), finish(), key_bytes(), read_key(), decode_pending_key(), FireIgnition, FireIgnitionId (+4 more)

### Community 188 - "AnchoredBlockEntity"
Cohesion: 0.13
Nodes (3): AnchoredBlockEntity, Catalog, Catalog

### Community 189 - "articulated.py"
Cohesion: 0.15
Nodes (10): convert(), geometry(), world(), multiply(), node_matrix(), source_path(), surfaces(), Glb (+2 more)

### Community 190 - "EntityDefinition"
Cohesion: 0.11
Nodes (3): EntityDefinition, EntityState, Catalog

### Community 191 - "recovery/tests.rs"
Cohesion: 0.26
Nodes (6): active_and_retiring_references_prevent_eviction_and_retry(), bundle(), metadata_corruption_after_pressure_does_not_restore_retired_memo(), ordinary_corruption_keeps_cache_and_retry_is_strictly_bounded(), pressure(), unused_cache_is_released_before_one_local_verification_retry()

### Community 192 - "script_startup/gameplay.rs"
Cohesion: 0.15
Nodes (8): Fixture, luau_action_loopback_rollbacks_exact_transfer_receipts_and_restart(), luau_action_planner_errors_and_unavailable_retry_are_atomic(), luau_action_registration_and_persisted_source_identity_fail_closed(), Peer, PROFILE, REGISTER, SOURCE

### Community 193 - "client/bundle.rs"
Cohesion: 0.15
Nodes (8): CACHE, DeadlineStream, install(), invalid(), receive(), receive_progress(), session_references_released(), TEST_CACHE_LOCK

### Community 194 - "sealed_neighborhood"
Cohesion: 0.13
Nodes (10): key_offset(), incoming(), bounced_mode_reflects_surface_color_without_leaking_into_default(), distant_streamed_roof_blocks_and_reopens_a_deep_shaft(), emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides(), mapped_glowstone_definition_supplies_emission_to_light_builder(), opening_a_roof_shaft_relights_the_cave(), plants_and_leaves_transmit_daylight() (+2 more)

### Community 195 - "Clock"
Cohesion: 0.11
Nodes (6): Capture, Clock, DOMAIN, publish(), ReadStamp, state_key()

### Community 196 - "parallel/tests.rs"
Cohesion: 0.21
Nodes (16): batch(), bounded_queue_reports_saturation_without_accepting_a_partial_job(), cancellation_skips_queued_work_and_marks_running_results_cancelled(), completed_jobs(), dependency_waves_can_commit_twice_in_one_tick_and_later_wave_sees_prior_result(), job_errors_and_panics_reach_the_barrier_and_the_pool_keeps_running(), key(), owner_at() (+8 more)

### Community 197 - "Proposal: one coherent gameplay API"
Cohesion: 0.11
Nodes (19): 10. Custom models: required direction, deferred work, 1. A small, powerful set of concepts, 2. Atomic operations are a convenience, not a burden, 3. Specialized APIs are optional conveniences, 4. Built-in gameplay uses the same boundary, 5. Distinguish genuinely different execution contexts, 6. Developer experience is part of the implementation, 7. Server-delivered mod packages (+11 more)

### Community 198 - "Sender"
Cohesion: 0.12
Nodes (8): channel(), Sender, Shared, State, background_backlog_cannot_fill_edit_capacity_and_invalidation_removes_queued_work(), hot_edit_coalesces_and_promotes_without_losing_background_progress(), job(), shutdown_wakes_idle_workers()

### Community 199 - "render/material.rs"
Cohesion: 0.19
Nodes (15): blend_opposite_pixels(), emission_strengths(), face_uv(), item_material_layer(), item_material_layer_for(), material_layer(), material_layer_for(), material_mips() (+7 more)

### Community 200 - "Adapter"
Cohesion: 0.17
Nodes (4): Adapter, error(), register(), spawn_clear()

### Community 201 - "script/gameplay.rs"
Cohesion: 0.12
Nodes (6): command_declaration(), command_schema(), Declaration, declarer(), registration(), ScriptHandler

### Community 202 - "Runtime"
Cohesion: 0.16
Nodes (4): count(), number(), own_key(), Runtime

### Community 203 - "thunder.rs"
Cohesion: 0.09
Nodes (17): Reverb, add_bands(), BANDS, Build, build_voice(), Echo, ECHOES, length() (+9 more)

### Community 204 - "ProfileCell"
Cohesion: 0.20
Nodes (3): Context<'_>, ProfileCell, WorldSnapshot<'_>

### Community 205 - "players/lifecycle.rs"
Cohesion: 0.14
Nodes (11): committed(), drive(), enqueue(), Job, joined(), key(), leaving(), Published (+3 more)

### Community 206 - "Update"
Cohesion: 0.19
Nodes (9): decode(), defaults(), Definition, identifier(), Kind, MAX_PARAMETERS, State, Update (+1 more)

### Community 207 - "client/startup.rs"
Cohesion: 0.32
Nodes (9): ascii(), display(), execute(), execute_event(), execute_retained(), identity(), prepare(), run() (+1 more)

### Community 208 - "VisualFire"
Cohesion: 0.14
Nodes (8): FireRenderer, FLOATS, MAX_BYTES, MAX_FIRES, triangle(), vertices(), VERTICES_PER_FIRE, VisualFire

### Community 209 - "render/tests.rs"
Cohesion: 0.11
Nodes (8): mesh_chunk(), adjacent_leaves_skip_interior_cutout_faces(), grass_side_is_upright_on_both_wall_axes(), greedy_quads_repeat_material_once_per_voxel(), mapped_builtin(), meshing_uses_shared_chunk_layout_and_world_origin(), plants_have_two_crossed_cutout_quads_and_do_not_hide_ground(), remapped_connection_catalog_drives_foliage_meshes_and_drop_art()

### Community 210 - "machine_component_tests.rs"
Cohesion: 0.19
Nodes (9): exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers(), independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart(), load_neighbours(), payload(), public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports(), pulse(), SelectiveMachine, settle() (+1 more)

### Community 211 - "character_asset.rs"
Cohesion: 0.12
Nodes (15): BODY_DEFINED_PNG, BODY_PNG, Channel, ChannelPath, CharacterVertex, CLEAN_FACE_PNG, Clip, EYE_NAMES (+7 more)

### Community 213 - "host-api/src/machine.rs"
Cohesion: 0.17
Nodes (10): FACES, Filter, Fuel, Machine, Port, Process, Slot, StackSelector (+2 more)

### Community 214 - "SystemDescriptor"
Cohesion: 0.05
Nodes (30): access(), AccessKind, BudgetKind, depends_on(), ExecutableSystem, F, IdentifierError, MAX_DEPENDENCIES_PER_SYSTEM (+22 more)

### Community 215 - "neighborhood.rs"
Cohesion: 0.16
Nodes (16): luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect(), luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes(), luau_neighborhood_radius_requires_exact_bounds_and_world_capability(), fixture(), GROW, id(), inbox(), load_neighborhoods() (+8 more)

### Community 216 - "script_startup/gameplay/profile_state.rs"
Cohesion: 0.18
Nodes (9): fixture(), lifecycle_profile_writes_compose_and_ambiguous_decisions_rollback_over_listener(), OFFLINE, profile_state_actions_are_atomic_owned_binary_and_restart_safe_over_listener(), profile_state_reads_reserve_existing_cells_and_absence_until_receipt(), profile_state_service_requires_package_capability_over_listener(), read_cell(), publish() (+1 more)

### Community 217 - "CoordinatorContext"
Cohesion: 0.21
Nodes (14): durable_actions(), fire_delivery(), fire_source(), input_authorization(), interaction_commit(), player_movement(), publish(), apply_simulation_input() (+6 more)

### Community 218 - "invoke"
Cohesion: 0.18
Nodes (8): Capabilities, captured_entity(), checked(), entity_identity(), invoke(), push_wake(), read_block(), cell()

### Community 219 - "MovingEntity"
Cohesion: 0.10
Nodes (11): Body, CollisionMask, MAX_ACCELERATION, MAX_LIFETIME_TICKS, MAX_SOURCE_EXCLUSION_TICKS, MAX_SPEED, MovingEntity, Response (+3 more)

### Community 221 - "wake.rs"
Cohesion: 0.14
Nodes (11): blocked(), canonical_wakes(), EntityWake, interact_producer(), INTERACT_PRODUCER_ID, MAX_WAKES_PER_PLAN, register_wake_kind(), route_wakes() (+3 more)

### Community 222 - "install"
Cohesion: 0.14
Nodes (6): install(), present(), install(), latch(), number(), view()

### Community 223 - "record.rs"
Cohesion: 0.28
Nodes (11): ExpiryReason, Impact, Motion, Pending, Projection, put_motion(), put_option_u64(), put_optional_target() (+3 more)

### Community 224 - "snapshots.rs"
Cohesion: 0.19
Nodes (9): apply(), dispatch(), finish(), MAX_SNAPSHOT_JOBS, Prepared, publish(), Selection, Target (+1 more)

### Community 225 - "Preparation"
Cohesion: 0.17
Nodes (3): Preparation, Ready, Renderer

### Community 226 - "Network"
Cohesion: 0.08
Nodes (13): ConfigWriter, connect_bundle_probe(), connect_catalog_probe(), connect_inventory_probe(), connect_ui_probe(), connect_visual_probe(), Incoming, Mesher (+5 more)

### Community 227 - "src/preview.rs"
Cohesion: 0.09
Nodes (11): CLIENT_MESH_RESULT_BATCH, CLIENT_PENDING_UPLOADS, DropPhase, FORMAT, MESHER_RESULT_CAPACITY, PERF_HEIGHT, PERF_RADIUS, PERF_STEADY_FRAMES (+3 more)

### Community 229 - "position_store.rs"
Cohesion: 0.19
Nodes (9): checksum(), invalid(), LEN, MAGIC, PositionStore, TEMP_SEQUENCE, validate_position(), validate_profile() (+1 more)

### Community 230 - "creature/services.rs"
Cohesion: 0.18
Nodes (9): coordinate(), guarded(), integer_cell(), invoke(), LifecycleRequest, parse_lifecycle(), parse_spawn(), SpawnRequest (+1 more)

### Community 231 - "draw"
Cohesion: 0.48
Nodes (3): choice(), color(), draw()

### Community 232 - "Context<'_>"
Cohesion: 0.29
Nodes (4): Context<'_>, Entity, EntityChange, EntitySpawn

### Community 233 - "durable/checkpoint.rs"
Cohesion: 0.24
Nodes (9): checkpoint_keys_turn(), decode_chunk_checkpoint_key(), decode_inventory_checkpoint_key(), fire_batch_key(), is_fire_checkpoint_key(), process_checkpoint_receipts(), submit_dirty_checkpoints(), submit_fire_checkpoint_batch() (+1 more)

### Community 234 - "bounded.rs"
Cohesion: 0.16
Nodes (6): expiry_due_prefix_is_capped_and_uncommitted_work_remains_eligible(), fill_chunk(), motion_is_scheduled_entity_work_never_coordinator_stepping(), one_moving_drop_costs_its_own_records_never_the_population(), staged_motion_bytes(), test_store()

### Community 235 - "script_startup/appearance.rs"
Cohesion: 0.19
Nodes (6): appearance_bundle_composes_with_rules_animation_and_exact_world_identity(), CALL, invalid_or_caught_appearance_declarations_never_publish(), Peer, registered_appearance_selection_replicates_and_restarts_by_profile(), source()

### Community 236 - "pipeline.rs"
Cohesion: 0.20
Nodes (6): create_custom_voxel_pipeline(), create_voxel_pipeline(), create_voxel_pipeline_source(), create_voxel_pipeline_with_catalog(), SHADER, VERTEX_STRIDE

### Community 237 - "WeatherSnapshot"
Cohesion: 0.17
Nodes (9): parse(), extreme_weather_clock_does_not_overflow_lightning_time(), Lightning, mix(), regional_strikes_are_stable_world_positions_near_far_and_negative_players(), transitions_are_continuous_and_lightning_is_shared(), WeatherKind, WeatherSnapshot (+1 more)

### Community 238 - "entity_sleep.rs"
Cohesion: 0.30
Nodes (19): process_durable_actions(), queue_interaction_actions(), dispatch(), edit(), empty_action(), live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery(), new_sleepers_cannot_extend_the_current_recheck_pass(), position() (+11 more)

### Community 239 - "model.rs"
Cohesion: 0.05
Nodes (32): FUEL_SLOT_INDEX, fuel_ticks(), INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS, KILN_MAX_FUEL_TICKS, KILN_MAX_PAYLOAD_BYTES, KILN_MAX_RECIPES, KILN_TICK_INTERVAL (+24 more)

### Community 240 - "Execution foundation: next implementation slices"
Cohesion: 0.12
Nodes (16): 1. Scheduling and progress under capacity pressure, 2. Reliable wake and sleep semantics, 3. Separate conflict revisions from publication ordering, 4. Consolidate commit orchestration and make barriers explicit, 5. Off-thread publication and bounded checkpoint work, Execution approach, Execution foundation: next implementation slices, First: review the current worker slice — done (+8 more)

### Community 241 - "server/appearance.rs"
Cohesion: 0.21
Nodes (7): checksum(), MAX_LEN, replace(), select(), select_character(), SEQUENCE, Store

### Community 242 - "script_startup.rs"
Cohesion: 0.19
Nodes (7): CONTENT, Fixture, luau_failed_restart_leaves_existing_save_unchanged_and_can_retry(), luau_startup_item_reaches_listener_inventory_and_restart(), luau_startup_rejections_publish_nothing_and_never_open_world(), luau_startup_validates_contracts_and_runs_imports_at_the_item_bound(), TOKEN

### Community 243 - "script_startup/bundle.rs"
Cohesion: 0.24
Nodes (15): bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join(), bundle_restart_exact_cache_and_changed_source_require_new_bytes(), client_verification_rejects_relay_tamper_truncation_and_reordering(), closed(), fixture(), fragmented(), offer(), package_effect_is_prepared_by_real_client_join_before_welcome() (+7 more)

### Community 244 - "ScriptError"
Cohesion: 0.17
Nodes (8): Reader<'a>, BUDGET, exhausted(), MAX_ARTIFACTS, MAX_BYTES, Reservation, reserve(), ScriptError

### Community 245 - "script/generation.rs"
Cohesion: 0.15
Nodes (7): coordinate(), Declaration, declarer(), invoke(), registration(), runtime(), ScriptContributor

### Community 246 - "BlockEditCommand"
Cohesion: 0.10
Nodes (17): block_intersects_player(), plan(), plan(), validate_player_credit(), plan(), BlockEditCommand, cached_block_or_request(), cached_block_with_reads() (+9 more)

### Community 247 - "authored.rs"
Cohesion: 0.12
Nodes (7): ATLAS_SIZE, Document, INVALID, json(), MAX_TEXT, Resources, Widget

### Community 248 - "src/composition.rs"
Cohesion: 0.10
Nodes (15): ACTIONS, ANCHORED_ENTITIES, Bundle, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, ITEM_ICONS (+7 more)

### Community 249 - "Patrol"
Cohesion: 0.17
Nodes (4): definition(), KEY, Patrol, State

### Community 250 - "Writer"
Cohesion: 0.10
Nodes (19): anchored_client_artifact_preserves_full_native_contract_and_catalog_identity(), anchored_client_artifact_rejects_storage_and_machine_ownership_collisions(), anchored_client_artifact_rejects_unresolved_refs_truncation_and_nested_wrappers(), artifact(), artifact_on(), base(), declaration(), decode_bytes() (+11 more)

### Community 251 - "client/lifecycle/tests.rs"
Cohesion: 0.26
Nodes (9): exercise_join_lifecycle(), exercise_player_services(), join(), player_notices_present_only_current_session_and_kicks_retire_it(), player_roster_accepts_newer_snapshots_and_is_cleared_when_session_retires(), public_player_snapshots_validate_session_and_services_then_clear_on_retirement(), read(), retired() (+1 more)

### Community 252 - "parse"
Cohesion: 0.12
Nodes (15): gpu_moving_projectile_flight_bounce_guidance_and_impact_filmstrip(), declarer(), field(), optional_integer(), optional_text(), owned(), parse(), state_key() (+7 more)

### Community 253 - "Cross-cutting integration findings"
Cohesion: 0.13
Nodes (15): Approved unified gameplay implementation, Audit boundary and live path, Baseline and verdict, Built-in capability parity audit, Cross-cutting integration findings, Evidence and integration acceptance, Existing capabilities versus new gameplay, F1 — Registered actions must replace the still-live kiln shortcut (+7 more)

### Community 254 - "package/tests.rs"
Cohesion: 0.25
Nodes (11): cycles_depth_and_shared_execution_budget_are_bounded(), dependency_versions_and_manifest_declarations_are_strict(), discovery_bounds_directory_count_source_bytes_and_total_bytes(), discovery_rejects_symlinks_at_every_path_level_and_special_files(), failed_modules_are_not_reinitialized_when_caught(), Fixture, import_failures_name_package_version_and_module_and_do_not_poison_worker(), imported_source_limits_and_exported_function_errors_keep_source_identity() (+3 more)

### Community 256 - "lighting.rs"
Cohesion: 0.19
Nodes (8): build_bounce(), index(), is_opaque(), MAX_LIGHT, PLANE, propagate(), SIDE, VOLUME

### Community 257 - "slot.rs"
Cohesion: 0.12
Nodes (6): draw(), draw(), color_from_swatch(), paint_icon(), show(), SlotStyle

### Community 258 - "State"
Cohesion: 0.14
Nodes (7): column_cover(), COVER_PERIOD, COVER_SIDE, flash_at(), scan_ceiling(), State, super::ClientApp

### Community 259 - "GenerationError"
Cohesion: 0.18
Nodes (3): GenerationError, Output, Charged

### Community 260 - "EntityTransferPolicy"
Cohesion: 0.07
Nodes (10): Adapter, Adapter, AutomationStack, EntityItemTransfer, EntityTransferPolicy, movable_count(), move_up_to(), PortRoute (+2 more)

### Community 262 - "Gpu"
Cohesion: 0.11
Nodes (4): Data, Gpu, MaterialData, updates_are_owned_typed_atomic_and_coalesced()

### Community 263 - "PlayerSummary"
Cohesion: 0.21
Nodes (7): complete(), normalize(), token(), PlayerSummary, read(), validate_notice(), write()

### Community 264 - "resolve_nodes"
Cohesion: 0.16
Nodes (13): identifier(), owned(), bounded_nodes(), control(), dynamic_control_forest_validates_choices_ownership_and_container_parents(), Kind, Presentation, raw() (+5 more)

### Community 265 - "FootprintCell"
Cohesion: 0.12
Nodes (8): FootprintCell, footprint(), list(), observe(), offset(), parse(), parse(), parse()

### Community 266 - "system/intents.rs"
Cohesion: 0.18
Nodes (12): luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id(), luau_intent_send_requires_opt_in_and_absence_requires_bootstrap(), CHAIN, id(), inbox(), luau_intent_absent_destinations_run_on_real_listener_and_recover_once(), luau_intent_caught_invalid_and_overbudget_sends_poison_all_output(), luau_intent_declarations_and_session_identity_are_owned_and_bounded() (+4 more)

### Community 267 - "Committed"
Cohesion: 0.13
Nodes (6): Committed, Observer, UseObserver, luau_committed_observer_timeout_cannot_block_receipt_or_later_observer_over_listener(), Witness, ScriptObserver

### Community 268 - "client/world.rs"
Cohesion: 0.22
Nodes (7): Cursor, MAX_PENDING_GROUPS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot, WorldProbe, WorldUpdate

### Community 269 - "registry/tests.rs"
Cohesion: 0.18
Nodes (19): builtin_phase_plan(), register_builtin_systems(), declarations_match_the_current_execution_shape(), descriptor(), deterministic_plan(), disjoint_and_read_only_accesses_can_share_a_phase(), duplicate_ids_and_invalid_namespaced_ids_are_rejected(), freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() (+11 more)

### Community 270 - "Hit"
Cohesion: 0.09
Nodes (27): no_hit(), no_interact(), player_adapter(), PLAYER_ENTITY_TYPE, project_avatar(), INSERT_FUEL, INSERT_INPUT, interact_verb() (+19 more)

### Community 271 - "Imports"
Cohesion: 0.11
Nodes (4): Imports, MAX_IMPORT_DEPTH, ModuleState, Compiled

### Community 272 - "OwnerApplyReceipt"
Cohesion: 0.22
Nodes (3): OwnerApplyReceipt, OwnerApplyTask, World

### Community 273 - "decode_transaction"
Cohesion: 0.20
Nodes (10): decode_transaction(), encode_frame(), frame_checksum(), invalid_data_owned(), invalid_input(), Reader, Reader<'a>, transaction_size() (+2 more)

### Community 274 - "entities/player.rs"
Cohesion: 0.21
Nodes (7): MAX_PLAYER_ENTITY_PAYLOAD_BYTES, MAX_SESSION_PLAYER_ENTITIES, PLAYER_ENTITY_TYPE, player_public_payload_codec_is_fixed_size(), player_type_registers_only_against_catalogued_identity(), PlayerEntityPayload, register_player_entity_type()

### Community 275 - "package/manifest.rs"
Cohesion: 0.18
Nodes (8): asset_path(), bounded_path(), identifier(), Manifest, public_path(), SourceSide, valid_path(), valid_version()

### Community 276 - "Observations"
Cohesion: 0.17
Nodes (11): ActionView, BlockView, ComponentView, InventoryView, key(), Observations, revision(), SlotView (+3 more)

### Community 277 - "script_startup/anchored.rs"
Cohesion: 0.14
Nodes (12): connect(), counter_total(), edit(), FIRST, luau_anchored_counter_cost_public_interaction_reaction_refund_and_restart_over_real_listener(), open(), packages(), prepare() (+4 more)

### Community 279 - "System"
Cohesion: 0.10
Nodes (16): DropSpawn, EntityChange, EntitySpawn, IntentDelivery, IntentId, IntentOutbox, IntentRequest, MAX_INTENT_PAYLOAD_BYTES (+8 more)

### Community 280 - "Scripting Gap Closure Plan"
Cohesion: 0.15
Nodes (13): Coverage and implementation order, Design requirements, Milestone 1 Authoring foundations, Milestone 2 Gameplay control and native parity, Milestone 3 World jobs and structured observations, Milestone 4 Dynamic UI, Milestone 5 Audio, Milestone 6 Models geometry and physics (+5 more)

### Community 282 - "ReplicationProbe"
Cohesion: 0.15
Nodes (9): ReplicationProbe, action(), action_result(), chest_collects_hopper_output_while_moving_and_building_over_real_tcp(), observe(), package_downloads_and_cancellations_preserve_live_movement_edits_and_machine_progress(), placement_probe(), running_hopper_feeds_kiln_while_player_moves_and_places_over_real_tcp() (+1 more)

### Community 286 - "check_articulated_clearance.py"
Cohesion: 0.18
Nodes (6): check(), main(), polys(), Poly, sat(), unique_axes()

### Community 287 - ".public_view"
Cohesion: 0.35
Nodes (4): AppearanceCodec, encode_stack(), StackPayload, StackPayloadCodec

### Community 288 - "atomic"
Cohesion: 0.08
Nodes (15): accepted_work_budget_has_explicit_nonblocking_overflow(), pre_edit_worker_result_is_rejected_after_uncheckpointed_edit(), receive_before(), requests_are_deduplicated_and_negative_chunks_load_asynchronously(), test_dir(), TEST_DIR_COUNTER, worker_load_uses_uncheckpointed_authoritative_snapshot_after_eviction(), NEXT (+7 more)

### Community 289 - "Command"
Cohesion: 0.30
Nodes (5): Command, CommandArgument, CommandValue, MAX_COMMAND_ARGUMENTS, CommandPermission

### Community 290 - "registered/tests.rs"
Cohesion: 0.39
Nodes (10): chunk(), emitted(), invalid_payload_and_duplicate_destinations_are_rejected(), keys(), oversized_expanded_fanout_aborts_routing_before_any_batch_is_returned(), registered_consumer_builds_a_typed_scratch_patch_for_its_destination(), registry(), routed_consumer_runs_once_with_stable_owner_kind_batch() (+2 more)

### Community 291 - "tests/client.rs"
Cohesion: 0.16
Nodes (9): consume(), exercise_player_teleport(), asset_aggregate_bytes_and_declaration_count_are_bounded(), asset_set_count_is_bounded_even_for_empty_files(), assets_use_secure_bounded_regular_file_reads(), classified(), classified_packages_preserve_clientless_startup_and_server_import_authority(), discovery_exports_only_classified_frozen_bytes_in_canonical_order() (+1 more)

### Community 292 - "runtime/memory.rs"
Cohesion: 0.29
Nodes (8): begin(), exceeded(), install(), memory_error(), memory_text(), observe(), reject(), Rejected

### Community 293 - "ItemIcon"
Cohesion: 0.16
Nodes (3): ItemIcon, definitions(), Catalog

### Community 294 - "Adapter"
Cohesion: 0.18
Nodes (3): Adapter, offset(), register()

### Community 295 - "character_asset/gameplay/tests.rs"
Cohesion: 0.15
Nodes (4): final_head_look_clamps_animation_and_input_to_the_hair_envelope(), mirrored_tools_move_both_elbows_and_wrists_and_return_without_a_pop(), planted_stance_and_clear_swing_feet_follow_actual_mesh_through_all_gait_blends(), relative()

### Community 296 - "Harvest"
Cohesion: 0.29
Nodes (3): Harvest, Pickup, PlantSupport

### Community 297 - "Mixer"
Cohesion: 0.12
Nodes (8): FRAMES, Limiter, QUEUE, UNIT, gains(), MAX_VOICES, Mixer, Voice

### Community 299 - "position_store/tests.rs"
Cohesion: 0.21
Nodes (5): corrupted_position_is_not_silently_replaced(), position_checkpoint_does_not_share_inventory_temp_namespace(), position_round_trips_and_is_profile_scoped(), SEQUENCE, TestSave

### Community 300 - "declarer"
Cohesion: 0.11
Nodes (6): declarer(), field(), number(), triple(), Declaration, declarer()

### Community 301 - "Inputs"
Cohesion: 0.15
Nodes (3): expand(), Expansion, Inputs

### Community 302 - "decode"
Cohesion: 0.12
Nodes (6): BUDGET, MAX_TRANSIENT_BYTES, Reservation, reserve(), simultaneous_verification_is_bounded_and_failure_releases_admission(), decode()

### Community 303 - "script/capacity.rs"
Cohesion: 0.10
Nodes (21): BLOCKS_PER_PACKAGE, CLIENT_PREPARATION_WALL_TIME, GENERATION_SCRIPT_WALL_TIME, GENERATORS_PER_PACKAGE, INSTALLATION_WALL_TIME, ITEMS_PER_PACKAGE, MAX_ASSET_BYTES, MAX_ASSETS (+13 more)

### Community 304 - "EntityDependencies"
Cohesion: 0.15
Nodes (3): EntityDependencies, EntityStore, PreparedEntityTransaction

### Community 305 - "Downloads"
Cohesion: 0.10
Nodes (4): complete_package_handshake(), Downloads, receive_package(), state_with_package()

### Community 307 - "SignalPost"
Cohesion: 0.21
Nodes (3): KEY, SignalPost, State

### Community 308 - "OwnerWorldView"
Cohesion: 0.11
Nodes (5): MotionContact, Context, OwnedEntity, WorldRead, OwnerWorldView

### Community 309 - "inventory/container.rs"
Cohesion: 0.24
Nodes (5): decode(), encode(), invalid(), max_bytes(), independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data()

### Community 310 - "Storage"
Cohesion: 0.25
Nodes (3): checksum(), SavedEdits, Storage

### Community 312 - "_"
Cohesion: 0.15
Nodes (12): _, BYTES_PER_ROW, FORMAT, HEIGHT, MAX_MEASURED_FRAMES, run(), run_character_benchmark(), scene() (+4 more)

### Community 314 - "src/client/tests.rs"
Cohesion: 0.12
Nodes (10): audio_controls_survive_character_settings_reconciliation(), block_edit_uses_selected_hotbar_block_and_hit_face(), confirmed_fire_visuals_expire_and_are_capped_and_distance_culled(), graphics_controls_apply_save_and_preserve_values_while_disabled(), lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently(), latest_edit_mesh_survives_a_superseded_kiln_relight_backlog(), mapped_server_item_and_replaceable_state_drive_placement_preview(), moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness() (+2 more)

### Community 315 - "gameplay_anchor_tests.rs"
Cohesion: 0.17
Nodes (10): action_and_tick_expand_secondary_cell_once_and_refund_final_inventory_on_restart(), AnchorExtension, barrier(), commit_command(), Destroy, GroundRemoved, Neighbor, open_anchor() (+2 more)

### Community 316 - "raycast_blocks"
Cohesion: 0.22
Nodes (7): aiming_past_grass_edges_reaches_ground_but_center_hits_flower(), integer_plane_moving_negative_starts_in_the_entered_voxel(), parallel_axis_uses_half_open_boundary_ownership(), raycast_blocks(), reports_target_face_and_adjacent_cell(), simultaneous_corner_crossing_advances_all_axes(), traverses_negative_coordinates_and_negative_faces()

### Community 318 - "Catalog"
Cohesion: 0.05
Nodes (53): ACTIVE, block_flags(), BlockDef, BlockTextures, BUILTIN_EMISSION, BUILTIN_FLAGS, BUILTIN_REFLECTANCE, Catalog (+45 more)

### Community 319 - ".spawn"
Cohesion: 0.15
Nodes (4): Lane, Mailbox, Runner, Snapshot

### Community 320 - "script_startup/gameplay/entities.rs"
Cohesion: 0.19
Nodes (7): Fixture, luau_entity_schema_loopback_spawn_due_callback_and_recovery(), luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling(), prepare(), REGISTER, REGISTER_ENTITY, SOURCE

### Community 321 - "render.rs"
Cohesion: 0.10
Nodes (7): DEPTH_FORMAT, MAX_PENDING_MESHES, SKY_COLOR, SUN_DIRECTION, UPLOAD_BYTES_PER_FRAME, UPLOAD_MESHES_PER_FRAME, VERTEX_FLOATS

### Community 323 - "prepare"
Cohesion: 0.29
Nodes (3): accepts(), capture(), prepare()

### Community 324 - "join"
Cohesion: 0.08
Nodes (38): failed_startup_queue_does_not_register_a_ghost_profile(), full_outbound_queue_disconnects_only_the_slow_client(), joined_players_spawn_above_solid_terrain_with_headroom(), joins_find_lower_safe_surface_after_origin_support_is_mined(), multiple_clients_receive_edit_delta_then_resync_snapshot_in_order(), remote_player_spawn_move_and_leave_publish_ordered_entity_changes(), startup_and_live_spawn_can_use_negative_ground_after_excavation(), streamed_interest_pins_release_on_resync_and_disconnect() (+30 more)

### Community 325 - "complete_barrier"
Cohesion: 0.32
Nodes (7): advance_receipts(), CommitBarrier, CommitProgress, complete_barrier(), drain_staged_receipts(), flush_ready_fire(), poll_journal_receipts()

### Community 326 - "build_stream"
Cohesion: 0.15
Nodes (8): build_stream(), open_device(), write_output(), audio_output_callback_converts_channels_and_silences_underruns(), audio_output_controls_are_coherent_bounded_and_queue_reset_is_guaranteed(), audio_output_reset_adoption_restores_explicit_post_reset_preview_controls(), audio_output_reset_discards_buffered_old_session_and_shutdown_is_silent(), controls()

### Community 328 - "queries/tests.rs"
Cohesion: 0.33
Nodes (7): airborne_count_tracks_schedule_not_records(), expired_drop_is_visible_but_never_pickable(), immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation(), pickup_delay_gates_candidates_but_not_visibility(), single_drop_collection_rechecks_range_delay_and_expiry(), spawn_direct(), test_store()

### Community 329 - "Player"
Cohesion: 0.17
Nodes (4): Context<'_>, Player, capture(), publish_roster()

### Community 330 - ".accept"
Cohesion: 0.13
Nodes (6): chunk_in_view(), ClientApp, lighting_depends_on(), mesh_priority(), ClientApp, ClientApp

### Community 331 - "2. Player and lifecycle hooks"
Cohesion: 0.06
Nodes (31): 1. Basic runtime tools — closed, 2. Player and lifecycle hooks, 3. Dynamic UI and input — closed within agreed scope, 4. General persistent block entities — closed, 6. Development iteration and save continuity, 7. Content-pack scale and composition limits, Acceptance and examples, Accepted implementation scope (+23 more)

### Community 332 - "EntityError"
Cohesion: 0.04
Nodes (56): checked_body(), crc32(), Decoder, Decoder<'a>, Encoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION, ENTITY_CELL_VALUE_MAGIC (+48 more)

### Community 333 - "actors/tests.rs"
Cohesion: 0.35
Nodes (10): avatar(), crouch_and_head_pitch_ease_and_teleport_resets_the_presentation_history(), fast_movement_cannot_accelerate_authored_walk_past_normal_playback(), ground_speed_blends_walk_and_run_but_stale_or_airborne_motion_decays(), interpolation_moves_between_samples_and_freezes_without_extrapolation(), landing_animation_follows_delayed_ground_contact_and_is_visual_only(), player_walk_blends_from_replicated_distance_then_stops_without_drift(), predicted_local_player_is_not_delayed_and_faces_the_current_look_heading() (+2 more)

### Community 334 - "script_startup/generation.rs"
Cohesion: 0.23
Nodes (9): Fixture, GENERATION, luau_generation_baseline_edits_and_identity_survive_restart(), luau_generation_rejects_bad_registration_and_caught_output_errors(), luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads(), luau_generation_streams_from_loader_after_restart(), MARKER, REGISTER (+1 more)

### Community 335 - "server/gameplay/entities.rs"
Cohesion: 0.37
Nodes (7): anchored(), nearby(), project(), read(), state(), validate_owner(), validate_state()

### Community 336 - "Config"
Cohesion: 0.08
Nodes (25): clamp_finite(), Config, CONFIG_VERSION, create_temporary_file(), MAX_FOV, MAX_SCALE, MAX_SENSITIVITY, MIN_FOV (+17 more)

### Community 337 - "streaming/entities.rs"
Cohesion: 0.19
Nodes (5): MAX_PUBLIC_ENTITIES_PER_CHUNK, MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK, project(), snapshot_messages(), SnapshotError

### Community 338 - "MirrorPermit"
Cohesion: 0.24
Nodes (8): apply_committed_action(), apply_committed_action_inner(), apply_committed_fire_action(), apply_committed_owner_world(), is_owner_publication_key(), publish_committed(), publish_committed_fire_after_world(), MirrorPermit

### Community 339 - "src/actions/tests.rs"
Cohesion: 0.21
Nodes (7): action(), command_facets_are_empty_gameplay_only_and_fingerprint_permissions(), composed_controls_resolve_forward_references_without_changing_target_context(), composition_bounds_and_fingerprint_cover_every_control(), discovery_is_bounded_ordered_and_rejects_conflicting_ownership(), empty_console_commands_do_not_consume_generic_action_capacity(), ordered_command_schema_has_canonical_bounded_arguments_and_identity()

### Community 341 - "rig.rs"
Cohesion: 0.17
Nodes (12): apply_look(), CHEST, clamp_look(), HEAD, LEFT_ARM, LEFT_LEG, NECK, PELVIS (+4 more)

### Community 342 - "tick/tests.rs"
Cohesion: 0.31
Nodes (10): drop_snapshot(), dropped_column_lands_at_rest_and_suspends(), falling_drop_integrates_exactly_one_fixed_step(), missing_terrain_defers_fail_closed_without_guessing(), neighbourhood_view(), neighbours(), NEXT_TEST_DIR, planner_rejects_anchored_locations_and_foreign_payloads() (+2 more)

### Community 343 - "player_services/tests.rs"
Cohesion: 0.23
Nodes (6): client_player_callback_rejection_is_atomic_and_other_packages_keep_running(), client_player_callbacks_compose_filter_public_state_and_disconnect_without_queue_room(), failed_client_import_initialization_is_cached_for_the_realm(), Fixture, retained_player_modules_keep_imports_and_coroutines_but_revoke_old_hosts(), states()

### Community 344 - "Bloxgloom interface plan"
Cohesion: 0.25
Nodes (8): Baseline when this plan was written, Bloxgloom interface plan, Delivery order, Goal, Interaction contract, Performance and correctness, UI and game-state design, Validation record

### Community 345 - "EntityCodecError"
Cohesion: 0.08
Nodes (13): BinCodec, CounterCodec, MateCodec, WideCodec, Codec, decode_stack(), encode_payload(), encode_stack() (+5 more)

### Community 346 - "reaction_removal_tests.rs"
Cohesion: 0.16
Nodes (6): drops(), open_soil(), RemovalDecision, SoilPost, SoilRegistration, stage_reaction()

### Community 347 - "nearest_unsent"
Cohesion: 0.23
Nodes (5): nearest_unsent(), boundaries_skip_unrepresentable_chunk_keys(), distance(), sent_keys_are_skipped_without_expanding_the_budget(), visits_every_interest_key_once_in_distance_order()

### Community 348 - "tests/anchored.rs"
Cohesion: 0.36
Nodes (7): owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record(), owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal(), removed(), seed_storage(), storage_startup(), STORE, unchanged()

### Community 349 - "prepare"
Cohesion: 0.18
Nodes (6): decode_motion_id(), invalid_data(), invalid_entity(), prepare(), PreparedEntityRecovery, validate_checkpointed_motion()

### Community 350 - "script_startup/player.rs"
Cohesion: 0.26
Nodes (8): check_movement(), custom_player_rules_negotiate_before_welcome_and_survive_restart(), FIELDS, player_artifact_tampering_is_rejected_or_fails_exact_manifest_match(), player_rules_and_drop_animation_share_one_verified_bundle_before_join(), player_rules_compose_with_existing_sized_item_artifacts(), rejected_player_declarations_including_caught_errors_never_open_world(), source()

### Community 351 - "owner_wave/tests.rs"
Cohesion: 0.44
Nodes (9): a_system_wave_cannot_commit_two_patches_for_the_same_owner(), batch(), chunk(), handler_or_budget_failure_aborts_the_entire_wave(), patch(), results(), system(), validated_wave_applies_in_canonical_owner_order_after_aggregate_checks() (+1 more)

### Community 352 - "ContentManifest"
Cohesion: 0.18
Nodes (10): checksum(), ContentEntry, ContentManifest, invalid(), MAGIC, MAX_ENTRIES, MAX_MANIFEST_BYTES, valid_state_key() (+2 more)

### Community 354 - "invalid"
Cohesion: 0.13
Nodes (15): cell_at(), checked(), entity_id(), invalid(), invoke_fields(), position_at(), state_bytes(), amount() (+7 more)

### Community 355 - "Bloxgloom"
Cohesion: 0.22
Nodes (9): Bloxgloom, Client execution and remaining extension work, Current execution architecture, Development and previews, HDR presentation, Publication and checkpoint boundaries, Run locally, Server threads and workers (+1 more)

### Community 356 - "process_movement_batch"
Cohesion: 0.31
Nodes (18): MovementCommand, process_movement_batch(), air_chunk(), close(), command(), command_order_is_stable_and_affects_the_authoritative_position(), crouch_geometry_budget_and_unknown_standing_are_authoritative(), invalid_and_excessive_deltas_consume_sequence_without_changing_position() (+10 more)

### Community 357 - "drop_merge.rs"
Cohesion: 0.24
Nodes (4): DropMergeCandidate, DropMergeContext, DropStackFill, filling_and_splitting_conserve_items_at_the_stack_cap()

### Community 358 - "colliders.rs"
Cohesion: 0.20
Nodes (8): capture(), collider(), History, intersects(), MAX_HISTORY_CREATURES, Pose, push(), sample()

### Community 359 - "arc"
Cohesion: 0.15
Nodes (9): unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers(), edit(), external_storage_lifecycle_seam_restart_retries_and_exact_refunds(), ids(), open(), resident(), startup(), registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes() (+1 more)

### Community 360 - "lifecycle-fixture/src/machine.rs"
Cohesion: 0.20
Nodes (5): Crush, KEY, MARKED_INPUT, REFINED_INPUT, register()

### Community 361 - "removal"
Cohesion: 0.10
Nodes (5): capacity_error(), Expansion, removal(), removal_cells(), removal_refunds()

### Community 362 - "mlua"
Cohesion: 0.08
Nodes (4): declarer(), FixedBytes, declarer(), ScriptBehavior

### Community 363 - "cold.rs"
Cohesion: 0.29
Nodes (11): ACTION, BEHAVIOR, body(), cold_resume_fixture(), dormant_fixture(), launch_fixture(), moving_real_listener_cold_terrain_and_dormancy_preserve_then_resume_saved_motion(), open_boxed() (+3 more)

### Community 364 - "EventRealm"
Cohesion: 0.17
Nodes (3): EventRealm, Export, load()

### Community 365 - "install"
Cohesion: 0.23
Nodes (8): field(), install(), optional_vector(), parse_change(), parse_spawn(), SpawnRef, table(), vector()

### Community 366 - "widgets.rs"
Cohesion: 0.32
Nodes (6): ControlValue, display_text(), nodes(), record(), sequence(), value()

### Community 367 - "script_startup/gameplay/player_inventory.rs"
Cohesion: 0.21
Nodes (12): concurrent_profile_inventory_transfers_retry_without_lost_items(), readonly_inventory_reservations_fence_writers_and_detect_stale_revisions(), corrupt_offline_inventory_rejects_caught_access_without_grant_or_server_failure(), cross_profile_inventory_requires_package_authority_even_for_reads(), fixture(), joined_cross_profile_inventory_and_state_commit_together_once(), offline_profile_tick_loads_asynchronously_and_commits_both_inventories(), oversized_profile_inventory_transaction_is_denied_and_next_wal_action_succeeds() (+4 more)

### Community 368 - ".first_solid_top"
Cohesion: 0.18
Nodes (4): FallingContext, FallingPlan, FallingWorld, Column

### Community 369 - "Registry"
Cohesion: 0.26
Nodes (3): key(), Registry, text()

### Community 370 - "journal/tests.rs"
Cohesion: 0.11
Nodes (18): append_direct(), all_incomplete_append_prefixes_recover_to_the_last_complete_record(), complete_corrupt_record_and_invalid_header_are_rejected(), exact_legacy_header_prefixes_are_repaired_and_nonprefixes_fail_closed(), replay_deduplicates_identical_ids_and_rejects_conflicting_reuse(), writer_acknowledges_only_a_synced_transaction_and_reopens_it(), incomplete_or_rejected_records_cannot_advance_the_recovered_clock(), shared_clock_survives_tail_recovery_rotation_and_lower_tick_records() (+10 more)

### Community 372 - "coder.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 373 - "validate_spawn_volume"
Cohesion: 0.13
Nodes (4): prepare(), moving_launch_fences_complete_body_and_uses_final_terrain_overlay(), validate_spawn_volume(), WorldSnapshot<'_>

### Community 374 - "coder-fast.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 375 - "script_startup/machine/components.rs"
Cohesion: 0.16
Nodes (7): LifecyclePlan, Machine, luau_component_machine_processes_exact_stack_over_listener_and_recovers(), luau_machine_component_options_reject_invalid_constants_before_save(), luau_machine_component_recipe_negotiates_exact_predicate_and_preservation(), luau_machine_present_input_exact_output_and_component_fuel_roundtrip(), source()

### Community 376 - "owner_durable/tests.rs"
Cohesion: 0.14
Nodes (16): open(), byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation(), capacity_defers_while_corruption_stops(), chunk(), commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing(), committed_waves_mark_active_and_update_the_due_index(), counter_store(), dropped_prepared_wave_changes_nothing() (+8 more)

### Community 377 - "InventoryWorkers"
Cohesion: 0.18
Nodes (3): inventory_worker(), InventoryLoadRequest, InventoryWorkers

### Community 379 - ".draw_node"
Cohesion: 0.24
Nodes (4): Intent, rgba(), Session, trim_bytes()

### Community 380 - "Insects"
Cohesion: 0.13
Nodes (6): Insects, place(), VOICES, Bus, Listener, Spatial

### Community 382 - "spawn.rs"
Cohesion: 0.49
Nodes (5): collides(), collides_cached(), request_missing(), spawn_position(), spawn_position_cached()

### Community 383 - "GpuPass"
Cohesion: 0.12
Nodes (3): Data, GpuPass, texture_entry()

### Community 384 - "script_startup/machine.rs"
Cohesion: 0.24
Nodes (13): luau_machine_footprint_negotiates_across_seam_and_restarts(), luau_machine_footprint_places_and_breaks_secondary_cell_over_listener(), luau_machine_negotiates_plans_and_restarts(), luau_machine_ports_and_transfer_work_negotiate_and_restart(), luau_machine_recipe_list_negotiates_filters_and_restarts(), luau_machine_recipe_list_rejects_overlapping_inputs_before_save(), luau_machine_rejects_invalid_ports_and_undeclared_transfer_work(), luau_machine_rejects_missing_capability_and_caught_invalid_recipe() (+5 more)

### Community 385 - "Result"
Cohesion: 0.35
Nodes (6): invalid_data(), read_world_metadata(), resolve_content_map_with(), verify_content_map_with(), verify_new_world_directory(), write_content_map()

### Community 386 - "fields_with_command"
Cohesion: 0.26
Nodes (10): block(), fields(), fields_with_command(), motion(), motion_contact(), moving_target(), captured_motion_contact_preserves_exact_readonly_target_and_revision(), every_public_removal_cause_preserves_its_exact_luau_context() (+2 more)

### Community 387 - "invoke"
Cohesion: 0.17
Nodes (5): admit(), invoke(), profile_state(), deadline(), prepare()

### Community 389 - "players/inventory.rs"
Cohesion: 0.31
Nodes (3): decode(), install(), publish()

### Community 392 - "resolve_player_movement"
Cohesion: 0.42
Nodes (4): MAX_MOVEMENT_STEPS_PER_AXIS, player_collides(), resolve_player_movement(), ResolveError

### Community 393 - "declarer"
Cohesion: 0.14
Nodes (3): identifier(), declarer(), Event

### Community 396 - "plan_observed_request"
Cohesion: 0.33
Nodes (5): denied(), encode_command_arguments(), plan(), plan_observed_request(), plan_request()

### Community 398 - "moving/lifecycle.rs"
Cohesion: 0.22
Nodes (10): ACTION, BEHAVIOR, install(), launch(), moving_failed_expiry_keeps_profile_inventory_and_drop_rewards_atomic(), moving_real_listener_admin_cancel_rejects_nonadmin_and_clears_stuck_reaction(), moving_real_listener_dynamic_targets_source_exclusion_and_exact_receipt_replay(), moving_real_listener_expiry_effect_is_once_and_replay_preserves_historical_id() (+2 more)

### Community 399 - "prepare_recovery"
Cohesion: 0.33
Nodes (6): Checkpoint, invalid(), prepare_recovery(), read(), replay(), save()

### Community 400 - "State"
Cohesion: 0.05
Nodes (13): sources(), energy(), habitat_sources_are_stable_under_scene_order_and_listener_movement(), insects_follow_habitat_daylight_and_rain_without_fake_sources(), SceneSampler, Habitat, MAX_RAIN_TILES, RAIN_MATERIALS (+5 more)

### Community 401 - "run_perf_benchmark_async"
Cohesion: 0.21
Nodes (7): PerfGpuMesh, PerfGpuSubmesh, PerfPhase, PerfSample, print_percentiles(), run_perf_benchmark_async(), surface_height()

### Community 402 - "streaming.rs"
Cohesion: 0.10
Nodes (19): OutboundClientSnapshot, can_stream_snapshot(), can_stream_snapshot_size(), inside_view(), apply(), disconnect(), Input, prepare() (+11 more)

### Community 403 - "render_previews_weather"
Cohesion: 0.24
Nodes (8): measure_ui_prepare(), preview_frame(), PreviewOutput, PreviewScene, render_previews_at(), render_previews_weather(), render_previews_with_packages(), sample_inventory()

### Community 404 - "integer"
Cohesion: 0.12
Nodes (9): Declaration, declarer(), field(), parse_recipe(), parse(), sequence_len(), slot_list(), integer() (+1 more)

### Community 406 - "notifications.rs"
Cohesion: 0.23
Nodes (4): Lane, MAX_EVENT_BYTES, QUEUE, view_entity()

### Community 407 - "render/effects/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_preview(), SHADER, verified_example_gpu_pass_survives_resize_and_grades_scene(), version_two_graph_composes_declared_inputs_and_parameters_on_gpu()

### Community 408 - "Behavior"
Cohesion: 0.22
Nodes (5): Behavior, Context, DownwardFlow, Plan, Processor

### Community 409 - "plan"
Cohesion: 0.25
Nodes (6): denied(), Invocation, plan(), read_target(), sight(), verify_reach()

### Community 410 - "Public dynamic-entity surface"
Cohesion: 0.29
Nodes (7): Behavior and movement, Bounds and remaining surfaces, External proof: Copperling, Interaction and presentation, Public dynamic-entity surface, Registration and identity, Verification

### Community 411 - "Bindings"
Cohesion: 0.13
Nodes (13): Action, allowed_key(), Bindings, letter(), NamedBindings, parse(), valid_action_key(), Binding (+5 more)

### Community 412 - "coordinates"
Cohesion: 0.21
Nodes (3): coordinates(), Event, Event<'a>

### Community 413 - "gameplay"
Cohesion: 0.11
Nodes (3): install(), Output, SpawnRef

### Community 414 - "client/admin.rs"
Cohesion: 0.17
Nodes (7): BINDING_ROWS_PER_PAGE, binding_targets(), BindingTarget, Command, parse(), parse_with_players(), registered()

### Community 415 - "render"
Cohesion: 0.45
Nodes (4): read_rgba_png(), render(), render_egui_previews(), render_package_egui_previews()

### Community 416 - "Archived plans and audits"
Cohesion: 0.50
Nodes (4): Archived plans and audits, Foundation and interface, Modding, Scripting

### Community 418 - "ClientApp"
Cohesion: 0.07
Nodes (9): ClientApp, ClientApp, ClientApp, digit_slot(), LightSample, eye(), perspectives_orbit_the_eye_and_cycle_without_changing_aim(), swept_camera_stops_before_walls_and_handles_close_or_unknown_cells() (+1 more)

### Community 419 - "ClientMessage"
Cohesion: 0.10
Nodes (13): ActionChoice, compose_named_command(), compose_observed_entity_action(), compose_package_action(), compose_package_action_with_args(), PackageActionInput, authored_entity_action_uses_observed_identity_and_exact_bounded_arguments(), PackageActionProbe (+5 more)

### Community 421 - "UiFrame<'_>"
Cohesion: 0.12
Nodes (6): draw(), CharacterPanel, UiDebug, UiFrame<'_>, .BINDING_VIEW_PREFIX, UiSettings

### Community 422 - "server/checkpoint/tests.rs"
Cohesion: 0.27
Nodes (14): capacity_counts_running_jobs_and_unconsumed_receipts(), closure_panic_becomes_an_error_receipt_and_worker_keeps_running(), completed_but_unconsumed_receipt_still_occupies_capacity(), drop_drains_accepted_work_without_blocking_on_full_receipt_channel(), failed_write_is_returned_with_its_key_and_revision(), independent_checkpoint_keys_progress_while_another_shard_is_blocked(), key(), key_on_shard() (+6 more)

### Community 424 - "join_worker.rs"
Cohesion: 0.19
Nodes (3): Control, Progress, JoinProgress

### Community 425 - "ObserverRegistration"
Cohesion: 0.20
Nodes (3): CommittedBlock, CommittedEntity, ObserverRegistration

### Community 428 - "run"
Cohesion: 0.43
Nodes (3): main(), parse_tint(), run()

### Community 430 - "custom/tests.rs"
Cohesion: 0.38
Nodes (4): gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry(), gpu_preview(), gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters(), GREEN

### Community 431 - "General anchored block entities"
Cohesion: 0.40
Nodes (5): Callback, Declaration, Example and checks, General anchored block entities, Scheduling, authority and publication

### Community 433 - "script_startup/moving.rs"
Cohesion: 0.18
Nodes (9): ACTION, BEHAVIOR, moving_failed_impact_remains_durable_and_does_not_grant_partial_reward(), moving_loopback_debits_once_sweeps_reacts_and_recovers_consumed_impact(), package(), prepare(), PROFILE, records() (+1 more)

### Community 434 - "register"
Cohesion: 0.31
Nodes (3): Chest, definition(), register()

### Community 435 - "quad"
Cohesion: 0.43
Nodes (3): linear_color(), quad(), Session

### Community 437 - "script_startup/gameplay/player_operations.rs"
Cohesion: 0.31
Nodes (5): committed_kick_closes_real_listener_session_and_reconnect_has_new_epoch(), fixture(), joined_lifecycle_can_send_notice_but_admission_cannot_stage_session_effects(), player_notices_require_package_authority_and_share_rollback_and_receipts(), send_collect()

### Community 439 - "crate"
Cohesion: 0.16
Nodes (5): animate(), apply(), capture(), slots(), stack()

### Community 440 - "Control"
Cohesion: 0.25
Nodes (3): Control, control_values_preserve_byte_limits_ranges_and_declared_choices(), SelectOption

### Community 441 - "DroppedItem"
Cohesion: 0.31
Nodes (5): DropAnimator, live_visual(), PickupFlight, POSITION_BLEND, DroppedItem

### Community 442 - "coder-smart.md"
Cohesion: 0.33
Nodes (5): Handoff, Implementation standard, Shared tree and commits, Start and scope, Verification

### Community 443 - "Phase 8: examples, parity and integrated verification"
Cohesion: 0.33
Nodes (6): Authoring and runnable examples, Integrated behavior and responsiveness, Phase 8: examples, parity and integrated verification, Production parity audit, Reproduce verification, Visual inspection and rendering context

### Community 444 - "native.rs"
Cohesion: 0.16
Nodes (5): fixture(), ImpactOnly, moving_native_generic_spawn_cannot_inject_a_valid_host_envelope(), moving_native_impact_only_reaction_resumes_physics_without_a_tick_callback(), PermissiveState

### Community 445 - "run_loop"
Cohesion: 0.35
Nodes (3): apply_motion(), run(), run_loop()

### Community 446 - "Agent guidance"
Cohesion: 0.50
Nodes (4): Agent guidance, Architecture and invariants, graphify, Verify graphics and performance

### Community 447 - "startup/moving/tests.rs"
Cohesion: 0.15
Nodes (4): Fixture, moving_decimal_minimum_extent_matches_native_f32_validation(), moving_startup_collects_frozen_body_and_three_handlers(), moving_startup_supports_private_model_free_entities()

### Community 448 - "manifest.json"
Cohesion: 0.22
Nodes (8): head_joint, joints, records, runtime_forward, runtime_scale, source_forward, source_height, version

### Community 449 - "remove"
Cohesion: 0.18
Nodes (4): invalid(), place(), remove(), spawn_effects()

### Community 451 - "shader"
Cohesion: 0.27
Nodes (5): palettes(), shader(), COMPUTE_FIXTURE, gpu_storm_fog_preserves_near_contrast_and_obscures_distant_shadows(), weather_fog_compute_fixture_validates_without_a_gpu()

### Community 452 - "resources.rs"
Cohesion: 0.19
Nodes (8): ArrayUsage, MAX_ARRAY_BYTES, MAX_ARRAY_LAYERS, required_limits(), counts_every_mipmap_and_enforces_the_array_byte_boundary(), requests_enough_device_layers_for_package_textures_and_native_materials(), target_package_texture_count_builds_a_valid_gpu_material_array(), validate()

### Community 453 - "EffectBuffer"
Cohesion: 0.23
Nodes (4): EffectBuffer, LIFE, MAX_EMBERS, FireStyle

### Community 455 - "public_systems/motion/tests.rs"
Cohesion: 0.18
Nodes (5): Bytes, catalog(), MotionOwner, owner_moving_spawn_wraps_record_and_rejects_missing_authority_or_capture(), spawn()

### Community 456 - "recipe_browser.rs"
Cohesion: 0.21
Nodes (7): activate(), AIM, change(), open(), PROFILE, recipe_browser_dynamic_controls_real_server_crafting_rollback_replay_and_restart(), settle()

### Community 457 - "client/appearance.rs"
Cohesion: 0.39
Nodes (3): apply_environment(), invalid(), parse()

### Community 458 - "simulation"
Cohesion: 0.29
Nodes (7): MAX_APPLY_JOBS_PER_BARRIER, MIN_OWNER_TASKS_PER_WORKER_GROUP, effect_patches_count_emissions_and_expose_their_replacement(), foreign_payloads_have_no_replacement(), plain_patches_carry_no_emissions_and_keep_their_replacement(), test_job(), test_owner()

### Community 461 - "Presentation"
Cohesion: 0.20
Nodes (4): MAX_STREAKS, MAX_VERTEX_BYTES, Presentation, random()

### Community 462 - "prepare_recovery"
Cohesion: 0.35
Nodes (7): decode_anchor(), invalid(), prepare_recovery(), read(), replay(), save(), Snapshot

### Community 465 - "bloxgloom"
Cohesion: 1.00
Nodes (3): bloxgloom, bloxgloom-host-api, bloxgloom-lifecycle-fixture

### Community 504 - "palette/tests.rs"
Cohesion: 0.43
Nodes (4): full_palette_reuses_removed_entry_and_preserves_other_cells(), row_copy_matches_flat_storage_in_each_mode(), state(), uniform_and_local_palette_boundaries_round_trip()

### Community 505 - "plan_inner"
Cohesion: 0.28
Nodes (8): clamp(), commit(), expire(), invalid(), plan(), plan_inner(), reaction(), solver_error()

### Community 507 - "commands.rs"
Cohesion: 0.24
Nodes (7): advertised_builtin_commands_and_compatibility_packets_share_auth_receipts_and_restart(), command_request(), declaration(), invalid_command_declarations_poison_startup_even_when_caught(), negotiated_commands_enforce_permission_and_zero_args_with_receipts_and_restart(), typed_mod_command_negotiates_order_validates_before_handler_and_recovers_once(), typed_request()

### Community 508 - "Preset"
Cohesion: 0.11
Nodes (13): Command, weather(), world_and_preview_sources_change_only_at_fade_boundary(), Preset, play_file(), play_preview(), render(), render_insect_preview() (+5 more)

### Community 509 - "duration"
Cohesion: 0.10
Nodes (11): authored_motion_uses_server_age_and_continues_into_partial_pickup(), item(), moving_drop_blends_between_authoritative_positions(), sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion(), app(), avatar(), breaking_starts_a_bounded_tool_animation_and_stance_requires_server_confirmation(), held_break_cancels_on_menus_capture_loss_and_session_retirement() (+3 more)

### Community 510 - "declarer"
Cohesion: 0.15
Nodes (4): articulated_recipes_replicate_independently_and_survive_server_restart(), Peer, declarer(), dense_array()

### Community 511 - "script_startup/gameplay/inventory.rs"
Cohesion: 0.21
Nodes (10): actor(), luau_automatic_pickup_exact_components_conservation_and_restart(), luau_component_schema_rejects_wrong_payload_and_persists_exact_bytes(), luau_inventory_exact_binary_reads_give_take_and_error_rollback_restart(), luau_pickup_host_credit_eligibility_permissions_and_caught_errors_rollback(), luau_take_and_spawn_stack_preserve_binary_components_across_receipt_and_restart(), PICKUP, PICKUP_REGISTER (+2 more)

### Community 514 - "PlayerDecision"
Cohesion: 0.10
Nodes (5): PlayerOperation, PlayerOperationKind, invoke(), PlayerDecision, apply()

### Community 515 - "admin/tests.rs"
Cohesion: 0.13
Nodes (4): generic_parser_uses_ordered_negotiated_schema_not_builtin_names(), player_command_names_completion_and_reconnect_use_exact_sessions(), request(), command_signature()

### Community 516 - "gameplay/admin.rs"
Cohesion: 0.18
Nodes (5): Admin, GIVE, SPAWN, TIME, WEATHER

### Community 517 - "src/appearance.rs"
Cohesion: 0.22
Nodes (7): BODIES, CHARACTER_RECIPE_BYTES, DEFAULT_HAIR_COLOR, EYES, HAIR, MAX_APPEARANCE_BYTES, MOUTHS

### Community 518 - "route_effects"
Cohesion: 0.36
Nodes (8): route_effects(), block_changes_fan_out_to_boundary_face_edge_and_corner_owners(), cell_effects_route_through_chunk_boundaries_and_euclidean_negative_coordinates(), chunk(), envelope(), local_and_cross_chunk_effects_share_the_interaction_commit_barrier_and_stable_order(), output_overflow_is_explicit_and_rejects_the_whole_producer_buffer(), route_rejects_mixed_ticks_duplicate_keys_and_excessive_limits()

### Community 519 - "app"
Cohesion: 0.47
Nodes (3): app(), declared_input_real_client_queues_once_opens_and_rebinds_without_gameplay(), declared_input_real_client_respects_screens_focus_modifiers_and_scope()

### Community 520 - "client_metadata.rs"
Cohesion: 0.18
Nodes (4): Catalog, Entity, Identity, Metadata

### Community 522 - "require"
Cohesion: 0.18
Nodes (9): Luau authoring in VS Code, Validate, Deterministic randomness, Diagnostics, Examples and compatibility, Libraries, Luau runtime tools, Runtime, delivery and save compatibility (+1 more)

### Community 523 - "Cache"
Cohesion: 0.25
Nodes (3): Cache, CAPACITY, Entry

### Community 524 - "Larger Luau packages and independent simulation features"
Cohesion: 0.22
Nodes (9): Deterministic terrain contributors, Example and acceptance, Final package measurements, Graphics verification and controls, Independent systems, Larger Luau packages and independent simulation features, Memory and decoded-resource admission, Shared admission policy (+1 more)

### Community 525 - "client/observations/tests.rs"
Cohesion: 0.35
Nodes (8): app(), inventory_zero_is_known_and_stale_or_duplicate_updates_do_not_replace_it(), only_contiguous_installed_deltas_publish_authoritative_cells(), pending(), receipts_retain_package_ownership_deduplicate_and_retire_with_the_session(), recent_cells_are_bounded_refreshed_by_snapshots_and_removed_on_eviction(), terrain_action_receipts_preserve_the_registered_package_key(), ui_less_callbacks_receive_latest_world_snapshot_as_readonly_data()

### Community 529 - "seed"
Cohesion: 0.29
Nodes (8): entity(), hex_id(), partition(), present(), profile(), seed(), word(), words()

### Community 531 - "Player lifecycle implementation"
Cohesion: 0.17
Nodes (12): Landed: committed observers, Landed: exact identity and action queries, Landed: general package-owned profile state, Landed: lifecycle state and scheduling, Landed: local public state and client lifecycle, Landed: runtime appearance, Landed: runtime teleport, Landed: targeted notices and session kicks (+4 more)

### Community 532 - "axis"
Cohesion: 0.40
Nodes (5): axis(), decode(), number(), position(), text()

### Community 533 - "PlayerState"
Cohesion: 0.42
Nodes (4): PlayerState, read(), valid_key(), write()

### Community 535 - "SpawnReceipt"
Cohesion: 0.44
Nodes (6): SpawnReceipt, MAX_ACTION_SPAWNS, read(), read_bytes(), validate(), write()

### Community 536 - "entities/moving_tests.rs"
Cohesion: 0.52
Nodes (6): catalog(), control_only_publications_advance_full_motion_without_advancing_wire_position_revision(), motion_only_replica_commits_allow_envelope_changes_and_preserve_public_state_revision(), moving_projection_validates_redundant_wire_pose_and_exposes_only_authored_public_bytes(), projected(), unchanged_motion_revision_cannot_change_pose_or_other_envelope_fields()

### Community 540 - "presentation/effects/tests.rs"
Cohesion: 0.38
Nodes (3): avatar(), colored_sparks_follow_the_same_bounded_session_lifetime(), embers_follow_only_presented_entities_expire_and_stay_bounded()

### Community 541 - "colliders/tests.rs"
Cohesion: 0.25
Nodes (4): Bytes, declaration(), moving_capture_uses_committed_creature_crossing_and_fences_target_motion(), record()

### Community 543 - "temp_save_dir"
Cohesion: 0.08
Nodes (57): anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic(), command(), edit(), fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record(), KEY, open(), public(), resident() (+49 more)

### Community 544 - "server/drops.rs"
Cohesion: 0.14
Nodes (8): DROP_RADIUS, GRAVITY, invalid(), is_drop_delta(), LIFETIME, TERMINAL_SPEED, VIEW_RANGE, VIEW_RANGE_SQ

### Community 545 - "decode"
Cohesion: 0.27
Nodes (3): decode(), encode(), Format

### Community 546 - "Modding: start here"
Cohesion: 0.50
Nodes (4): Historical design and audit, Modding: start here, Rust extension and host reference, Try authoring now

### Community 552 - "visual_contracts.rs"
Cohesion: 0.33
Nodes (6): copy(), fixture(), installed_authoritative_entity_replica_drives_the_shader_parameter(), invalid_typed_startup_parameter_refuses_real_join_with_source_identity(), negotiated_visuals_parameters_switch_and_restart_without_global_state(), open_boxed()

### Community 555 - "src/storage/tests.rs"
Cohesion: 0.29
Nodes (9): active_world_lock_excludes_a_second_writer_and_releases_on_drop(), content_map_preserves_wide_assignments_and_rejects_reassignment(), hidden_partial_conversion_stage_cannot_be_opened_even_after_marker_removal(), incomplete_conversion_cannot_be_opened_as_a_world(), old_world_is_rejected_without_creating_a_lock_or_rewriting_data(), temporary_root(), wide_sparse_edits_round_trip_and_reject_reordering(), with_extra_block() (+1 more)

### Community 556 - "Chunk"
Cohesion: 0.27
Nodes (3): predict_player_movement(), predict_player_movement_with_stance(), Chunk

### Community 557 - "package"
Cohesion: 0.28
Nodes (8): luau_anchored_callbacks_accept_full_registered_binary_limits_and_immutable_inputs(), luau_anchored_rejects_failed_validation_oversized_public_and_ambiguous_reaction(), luau_anchored_invalid_interaction_and_excess_refund_preserve_state_over_real_listener(), luau_anchored_registration_requires_capability_and_rejects_caught_invalid_geometry_before_save(), package(), sources(), luau_anchored_registration_rejects_unknown_sources_fields_and_out_of_range_bounds(), luau_anchored_then_storage_or_machine_same_block_rejects_caught_ownership_collision()

### Community 558 - "Registered content and composition"
Cohesion: 0.29
Nodes (7): Bounds, atomicity and compatibility, Composition and deterministic resolution, Existing capabilities exposed, Integration additions, Luau package capacities, Registered content and composition, Verification and limits

### Community 559 - "load.rs"
Cohesion: 0.15
Nodes (7): ACTION, count(), moving_real_listener_capacity_measurements(), package(), rank(), REGISTER, request()

### Community 561 - "EffectKindId"
Cohesion: 0.19
Nodes (3): EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen

### Community 562 - "SCRIPTING.md"
Cohesion: 0.07
Nodes (24): Chunk generation for native contributors, Luau contributor composition, Authored UI widgets and current limits, Exact identities and time, Larger packages, Local Luau packages, Package shape, Try the UI example (+16 more)

### Community 563 - "crouch.rs"
Cohesion: 0.52
Nodes (4): connect(), crouch_loopback_cannot_stand_in_ceiling_and_late_join_observes_posture(), movement(), stance()

### Community 564 - "create_target_pipeline"
Cohesion: 0.20
Nodes (3): create_target_pipeline(), target_outline_vertices(), TARGET_SHADER

### Community 566 - "invalid"
Cohesion: 0.09
Nodes (17): decode(), encode(), decode(), encode(), float(), decode(), encode(), float() (+9 more)

### Community 569 - "memory/tests.rs"
Cohesion: 0.48
Nodes (5): memory_errors_latch_before_handlers_transform_them_and_reset_explicitly(), protected_calls_can_yield_and_resume_without_a_rust_boundary(), protected_calls_preserve_values_and_normal_errors(), real_allocator_error_caught_in_pcall_is_latched(), runtime()

### Community 571 - "system/decisions.rs"
Cohesion: 0.38
Nodes (5): caught_invalid_burn_removal_decision_rejects_whole_owner_wave(), luau_burn_removal_context_and_drop_commit_with_owner_receipt(), OWNER, package(), REGISTER

### Community 573 - "target_actions.rs"
Cohesion: 0.25
Nodes (4): FIRST, PROFILE, SECOND, THIRD

### Community 575 - "materials.rs"
Cohesion: 0.29
Nodes (7): bound_texture_asset_must_decode_and_match_owned_canonical_metadata(), bundle(), DESCRIPTOR, material_bundle_verifies_key_ownership_limits_and_catalog_readiness(), SHADER, texture_metadata(), version_two_material_verifies_hooks_parameters_and_negotiated_layers()

### Community 578 - "Authoritative moving entities"
Cohesion: 0.14
Nodes (11): Acceptance evidence, Admission limits, Authoritative moving entities, Behavior events, Compatibility and presentation, Declaration, Gameplay services, Integration, reactions and persistence (+3 more)

### Community 579 - "declarations/moving/tests.rs"
Cohesion: 0.36
Nodes (6): artifact(), base(), declaration(), decode_bytes(), moving_metadata_preserves_native_catalog_and_has_inert_codec(), moving_metadata_rejects_missing_capability_overflow_and_nesting()

### Community 580 - "Slots"
Cohesion: 0.19
Nodes (3): Slots, StoragePayload, StoragePayload<N>

### Community 581 - "prepare"
Cohesion: 0.15
Nodes (3): Capture, prepare(), WorldSnapshot<'_>

### Community 582 - "decode"
Cohesion: 0.33
Nodes (6): axes(), decode(), encode(), MAGIC, word(), wrap()

### Community 583 - "CharacterRenderer"
Cohesion: 0.09
Nodes (10): CharacterRenderer, authored_gpu_character_draws_textured_animated_geometry_and_instance_tint(), both_bodies_all_hairstyles_support_independent_rgb_without_neighbor_changes(), different_recipes_color_only_selected_irises_and_swap_hair_per_instance(), gpu_registered_player_palettes_preserve_default_and_color_all_three_parts(), HEIGHT, render(), render_avatars() (+2 more)

### Community 588 - "audio/SCRIPTING.md"
Cohesion: 0.22
Nodes (6): Client presentation commands, Packaged and scripted audio, Server gameplay calls, Verification — October 1, 2026, voice(), Audio timer machine

### Community 592 - "inventory"
Cohesion: 0.32
Nodes (3): settle(), text(), typed_recipe_browser_uses_exact_inventory_components_clock_and_own_receipts()

### Community 593 - "Articulated characters"
Cohesion: 0.29
Nodes (6): Appearance, Articulated characters, Color contract, Geometry and movement, Rebuild and verify, Save and wire boundary

### Community 594 - "declarations/budget.rs"
Cohesion: 0.25
Nodes (5): block_bytes(), MAX_BYTES, MAX_COUNT, package_bytes(), tag_bytes()

### Community 595 - "record"
Cohesion: 0.42
Nodes (6): apply_command(), declaration(), read(), read_contact(), record(), spawn_record()

### Community 596 - "Game weather foundation"
Cohesion: 0.22
Nodes (9): Acceptance evidence, Authority, timing and persistence, Depth-fog acceptance, Game weather foundation, Integration with articulated characters, Material and insect sound integration, Presentation and bounds, Storm presentation follow-up (+1 more)

### Community 597 - "sample"
Cohesion: 0.27
Nodes (3): material(), sample(), sample_tracks_canopy_ground_edits_and_unknown_chunks()

### Community 598 - "ServerMessage"
Cohesion: 0.19
Nodes (9): BundleIdentity, CLIENT_RUNTIME_VERSION, MAX_BUNDLE_PART, read_identity(), read_part(), validate_part(), write_identity(), write_part() (+1 more)

### Community 600 - "presentation/observations/tests.rs"
Cohesion: 0.60
Nodes (5): committed_spawn_ordinals_expose_exact_readonly_entity_handles(), known(), observations_distinguish_unknown_from_known_empty_and_filter_action_ownership(), observations_keep_exact_binary_revisions_and_nested_views_readonly(), observations_reject_invalid_dense_inventory_components_world_and_window_bounds()

### Community 601 - "src/storage.rs"
Cohesion: 0.15
Nodes (11): CONTENT_MAP, CONVERSION_INCOMPLETE, FORMAT_VERSION, HEADER_LEN, MAGIC, MAX_SNAPSHOT_BYTES, SAVE_FORMAT_VERSION, TEMP_COUNTER (+3 more)

### Community 602 - "DropPolicy"
Cohesion: 0.22
Nodes (3): DropPolicy, .BYTE_LEN, .MAX_PICKUP_RANGE

### Community 604 - "FireAnimator"
Cohesion: 0.23
Nodes (4): FireAnimator, LIFE, MAX_DISTANCE_SQUARED, MAX_FIRES

### Community 606 - "outbound/tests.rs"
Cohesion: 0.43
Nodes (6): aggregate_byte_limit_is_enforced_across_clients(), aggregate_high_water_mark_survives_sub_tick_queue_drain(), frame_admission_includes_the_frame_currently_being_written(), per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop(), pong(), shared_encoding_keeps_independent_byte_reservations_until_each_client_releases()

### Community 607 - "Reaction"
Cohesion: 0.33
Nodes (5): Reaction, bytes(), invalid(), parse(), Reply

### Community 608 - "Appearance"
Cohesion: 0.11
Nodes (8): Appearance, MAX_ADDITIONS, MODEL, PANTS, SHIRTS, SKINS, decode(), encode()

### Community 610 - "world_time/tests.rs"
Cohesion: 0.39
Nodes (5): clock_command_recovers_past_an_older_checkpoint_and_rotated_wal(), local_listener_delivers_shared_world_time_admin_changes_and_recovers_it(), save(), temporary(), world_time_resumes_saved_phase_and_rejects_corrupt_state()

### Community 611 - "EffectBuffer"
Cohesion: 0.42
Nodes (4): boundary_coordinates(), EffectBuffer, EffectBufferError, EffectEnvelope

### Community 614 - "journal/recovery.rs"
Cohesion: 0.30
Nodes (7): Journal, legacy_header(), open_or_create_legacy(), sync_parent(), truncate_tail(), validate_legacy_header(), write_legacy_header()

### Community 616 - "state"
Cohesion: 0.39
Nodes (5): default_articulated_preview_preserves_palettes_and_clean_drafts_follow_server_changes(), draft_cancel_apply_echo_and_duplicate_apply_are_distinct(), implicit_default_is_editable_without_a_model_toggle_or_phantom_changes(), state(), unknown_snapshot_and_rejected_draft_cannot_apply_and_disconnect_clears_state()

### Community 619 - "coordinator/tests.rs"
Cohesion: 0.40
Nodes (6): acknowledged_result_stays_retired_across_rotation_and_restart(), grant(), inventory_action(), poll_until_settled(), temp_save_dir(), wal_replay_keeps_result_and_world_effect_before_checkpoint()

### Community 623 - "Luau VM lifetime and module state"
Cohesion: 0.29
Nodes (7): Choose the right state, Contexts and coroutines, Failures and teardown, Initialization and random streams, Luau VM lifetime and module state, Runnable example and measurement, Verification

### Community 624 - "Budget"
Cohesion: 0.30
Nodes (5): Budget, cached_shared_artifacts_and_rejected_growth_release_exact_admission(), compact_artifacts_cannot_hide_metadata_from_process_admission(), metadata_admission_preserves_typed_pressure_with_declaration_context(), wrapper_growth_keeps_owned_declarations_accounted()

### Community 625 - "tcp/report.rs"
Cohesion: 0.27
Nodes (5): TransportSnapshot, OutboundSnapshot, percentile(), summarize(), TcpSoakReport

### Community 626 - "Procedural"
Cohesion: 0.09
Nodes (8): lightning_hit(), Procedural, bell(), Cell, smooth(), Storm, Weather, Wind

### Community 628 - "stack"
Cohesion: 0.18
Nodes (9): advance(), chest_hopper_chest_chain_preserves_last_slot_components_restart_and_refunds(), contents(), resident(), transfer(), hopper(), hopper_feeds_kiln_collects_output_and_survives_restart_then_break(), pulse() (+1 more)

### Community 629 - "script_startup/machine/variants.rs"
Cohesion: 0.60
Nodes (3): luau_machine_variants_negotiate_place_from_finite_inventory_and_recover(), luau_machine_variants_reject_duplicate_and_foreign_states_before_save(), source()

### Community 630 - "deviceext"
Cohesion: 0.10
Nodes (3): array(), decode(), CharacterPreview

### Community 632 - "std"
Cohesion: 0.10
Nodes (11): DECODED_BYTES, MAX_CLIP_FRAMES, MAX_DECODED_BYTES, MAX_FILE_BYTES, builtin(), builtin_cached(), prepare(), wal_reservation_accounts_for_queued_frames_before_the_worker_sees_them() (+3 more)

### Community 634 - "validate"
Cohesion: 0.80
Nodes (3): dword(), validate(), word()

### Community 635 - "durable/fire/tests.rs"
Cohesion: 0.35
Nodes (6): check_seeded_fire_restart(), checkpoint_pressure_admits_a_durable_owner_prefix_without_losing_the_remainder(), dirty_fire_keys_use_one_checkpoint_job_and_revision_fenced_receipt(), drain_wal(), fire_burn_uses_public_removal_and_support_handlers_in_one_receipt(), temp_save()

### Community 636 - "MovementState"
Cohesion: 0.15
Nodes (4): MovementState, broadcast(), clear(), resolve()

### Community 638 - "world"
Cohesion: 0.07
Nodes (23): invoke_hook(), edit_command(), hook_chunk_requests_drain_even_when_planning_defers(), hook_stages_edits_through_builder_without_touching_live_world(), ok_hook_survives_failing_chunk_request(), temp_save_dir(), several_cells_in_one_chunk_publish_one_atomic_commit_part(), temp_save_dir() (+15 more)

### Community 640 - "content/moving/tests.rs"
Cohesion: 0.33
Nodes (4): Bytes, declaration(), maximum_authored_state_with_long_pending_contact_fits_durable_envelope(), moving_catalog_bindings_follow_saved_identity_remapping()

### Community 641 - "exposure"
Cohesion: 0.25
Nodes (4): exposure(), hillside(), hillside_opening_produces_audible_rain_through_the_game_mixer(), open_hillside_entrance_is_audible_and_closing_it_restores_muffling()

### Community 642 - "Extension"
Cohesion: 0.09
Nodes (13): Extension, Fixture, KEY, TallStore, HarvestExtension, NoPickupExtension, PlacementExtension, SupportExtension (+5 more)

### Community 643 - "visibility.rs"
Cohesion: 0.27
Nodes (5): chunk_visible(), chunk_visible_padded(), create_depth(), outside_clip(), view_projection()

### Community 644 - "Registered inventory views and screens"
Cohesion: 0.33
Nodes (6): Generic client and server paths, Independent persistence and bounds, Public registration, Registered inventory views and screens, Try the external fixture, Verification

### Community 646 - "passes"
Cohesion: 0.60
Nodes (3): after_dependencies_use_the_same_cycle_and_missing_checks(), inputs_override_order_and_invalid_graphs_keep_resource_identity(), passes()

### Community 745 - "lifecycle-fixture/src/content.rs"
Cohesion: 0.20
Nodes (6): CHIP, Content, LAMP, PNG, REED, TEXTURE

### Community 747 - "extension_system.rs"
Cohesion: 0.47
Nodes (3): clock(), external_system_runs_without_entities_and_recovers_across_real_listener_restart(), startup()

### Community 748 - "character_asset/gameplay.rs"
Cohesion: 0.22
Nodes (10): animate(), blend(), CROUCH_HIP_DEGREES, LEG_LENGTH, overlay(), rotate(), smooth(), tool_duration() (+2 more)

### Community 750 - "Articulated renderer performance"
Cohesion: 0.50
Nodes (3): Articulated renderer performance, Measurements, Terrain-only comparison

### Community 753 - "Content"
Cohesion: 0.50
Nodes (4): Content, Items, stack components and drops, Tags, Textures and blocks

### Community 755 - "entities/motion.rs"
Cohesion: 0.25
Nodes (7): DT, MAX_BODIES, MAX_CHUNK_BODIES, MAX_COLLIDERS, MAX_DYNAMIC_COLLIDERS, MAX_SWEEP_CELLS, STEP_TICKS

### Community 756 - "plan_event"
Cohesion: 0.44
Nodes (3): is_registered(), plan(), plan_event()

### Community 757 - "listener_chain"
Cohesion: 0.83
Nodes (3): durable_intent_bootstrap_chain_runs_through_real_listener_and_recovers_once(), durable_intent_chain_runs_through_real_listener_and_recovers_once(), listener_chain()

### Community 759 - "Accepted goal: authoritative moving entities and projectiles"
Cohesion: 0.20
Nodes (10): 5. Flexible entities, motion and presentation, Acceptance evidence, Accepted goal: authoritative moving entities and projectiles, Actual problems to solve, Author surface and ownership, Capacity, performance and implementation slices, Impact transactions, unload and restart, Implemented extension: packaged and scripted audio (+2 more)

### Community 760 - "src/motion/tests.rs"
Cohesion: 0.52
Nodes (6): contact_query_tracks_captured_motion_revision_and_absence(), expiry_roundtrip(), pending_reaction_roundtrip_and_truncations(), public_projection_keeps_private_state_separate(), record(), reject_noncanonical_pose_and_impact_identity()

### Community 762 - "write_frame"
Cohesion: 0.22
Nodes (3): bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix(), byte_budget_stops_turns_and_file_limit_does_not_write_overshoot(), write_frame()

### Community 763 - "server/weather/tests.rs"
Cohesion: 0.39
Nodes (5): durable_apply_does_not_rewind_elapsed_weather_time(), real_listener_synchronizes_admin_weather_and_denies_other_players(), severity_transitions_remain_continuous_and_survive_checkpoint(), temporary(), weather_restart_preserves_transition_and_wal_recovers_unapplied_override()

### Community 765 - "script/tests.rs"
Cohesion: 0.54
Nodes (6): elapsed_deadline_is_reported_with_module_identity(), input(), instruction_and_source_limits_reject_bad_modules_without_poisoning_worker(), memory_limit_and_syntax_error_are_attributable(), module(), sandbox_excludes_native_io_and_attributes_errors()

### Community 769 - "session_ids/tests.rs"
Cohesion: 0.53
Nodes (4): directory(), session_boot_ranges_preserve_first_ids_and_never_reuse_a_durable_player_reference(), session_reservations_are_serialized_and_corrupt_or_exhausted_counters_are_rejected(), session_server_restart_installs_a_new_range_before_player_admission()

### Community 775 - "transfer"
Cohesion: 0.33
Nodes (3): Work, parse(), transfer()

### Community 778 - "Registered anchored behavior"
Cohesion: 0.40
Nodes (5): Authority, scheduling and recovery, Bounds and limits, Public contract, Registered anchored behavior, Verification

### Community 781 - "Registered actions and composed controls"
Cohesion: 0.40
Nodes (5): Bounded composition, Negotiated commands, Production path and authority, Registered actions and composed controls, Supported targets and effects

### Community 782 - "Public storage lifecycle boundary"
Cohesion: 0.33
Nodes (6): Chest integration, Explicit limits / next work, Lifecycle contract, Package boundary, Public storage lifecycle boundary, Verification

### Community 783 - "Registered inventory machines"
Cohesion: 0.40
Nodes (5): Contract, Registered inventory machines, Remaining boundaries, Trying the fixture, Verification

### Community 791 - "server/movement/teleport.rs"
Cohesion: 0.32
Nodes (3): ready(), Reset, teleport()

### Community 792 - "showcase.rs"
Cohesion: 0.29
Nodes (3): phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart(), PRESS_ANCHOR, send_action()

### Community 793 - "bundle_ui.rs"
Cohesion: 0.13
Nodes (13): event(), authored_button_reaches_authoritative_receipt_and_durable_inventory(), client_startup_failure_refuses_content_ready_with_package_and_module(), downloaded_client_startup_is_session_scoped_across_reconnect_and_switch(), downloaded_replica_visuals_use_exact_entity_ids_and_reset_on_switch(), startup_fixture(), startup_worker_discards_partial_registration_and_caught_limit(), startup_worker_imports_exact_direct_dependencies_with_lexical_visibility() (+5 more)

### Community 797 - "tests/effects.rs"
Cohesion: 0.33
Nodes (4): bundle(), DESCRIPTOR, SHADER, verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures()

### Community 799 - "receive_content_manifest"
Cohesion: 0.17
Nodes (10): client_commands_are_rejected_until_content_ready_matches(), complete_content_handshake(), join_cleanup_enqueues_one_leave_with_the_next_sequence(), local_server_shutdown_restores_authoritative_position_on_next_start(), nonblocking_listener_streams_and_recovers_a_wal_acked_edit_inner(), production_reactor_joins_and_commits_an_edit_over_real_tcp(), receive_content_manifest(), farming_scale_cold_and_cached_join_latency() (+2 more)

### Community 800 - "Client audio foundation"
Cohesion: 0.18
Nodes (11): Acceptance measurements, Client audio foundation, Mixer and native playback contract, NoiseMachine adaptation, Open shelter entrances, Packaged scripting extension, Spatial insect ambience, Try it (+3 more)

### Community 802 - "avatars/moving_tests.rs"
Cohesion: 0.31
Nodes (3): catalog(), gpu_rigid_moving_model_rotates_in_three_dimensions_without_creature_deformation(), State

### Community 806 - "receipts/tests.rs"
Cohesion: 0.39
Nodes (5): ack_retires_both_outcomes_without_reopening_sequence(), bounded_window_can_run_beyond_old_lifetime_cap(), committed_launch_ids_survive_restart_and_duplicate_action_admission(), reconnect_closes_old_epoch_and_preserves_monotonic_epoch_after_codec(), record()

### Community 808 - "present"
Cohesion: 0.48
Nodes (3): entity(), present(), seed()

### Community 809 - "modding/README.md"
Cohesion: 0.16
Nodes (8): Growth foundation: remaining implementation, Luau package composition, Public persistent owner systems, Documentation, Persistent anchored counter, Durable neighborhood relay, Targeted authored UI fixture, Third-party source adaptations

### Community 810 - "register"
Cohesion: 0.33
Nodes (3): definition(), KEY, register()

### Community 811 - "menus/tests.rs"
Cohesion: 0.40
Nodes (3): graphics_settings_do_not_offer_a_model_override(), label_center(), native_character_menu_keeps_apply_visible_and_blocks_repeat_while_pending()

### Community 812 - "vm_latency.rs"
Cohesion: 0.40
Nodes (3): CALLBACK, report(), vm_lifetime_mixed_listener_latency()

### Community 813 - "metadata"
Cohesion: 0.47
Nodes (3): metadata(), observer_metadata_is_inert_bounded_and_rejects_nested_envelopes(), runtime_metadata_rejects_unknown_shapes_and_unbounded_counts_with_valid_digest()

### Community 816 - "actors/moving/tests.rs"
Cohesion: 0.60
Nodes (5): avatar(), codec_normalization_tolerance_cannot_break_quaternion_interpolation(), committed_ticks_interpolate_orientation_without_gait_or_extrapolation(), impact_corrects_immediately_and_stale_motion_cannot_resurrect_flight(), model_replacement_and_removal_drop_retained_motion_history()

### Community 821 - "active.rs"
Cohesion: 0.53
Nodes (4): luau_machine_active_state_negotiates_and_preserves_save_identity(), luau_machine_active_state_rejects_foreign_and_unpowered_states(), luau_machine_fuel_switches_authored_state_over_listener_and_recovers(), source()

### Community 823 - "replant.rs"
Cohesion: 0.40
Nodes (3): MAX_REPLANT_CELLS, OWNER_PROBES_PER_TICK, Replanter

### Community 825 - "weather/codec.rs"
Cohesion: 0.33
Nodes (3): BYTES, decode(), encode()

### Community 826 - "Dynamic authored UI and input"
Cohesion: 0.40
Nodes (5): Callback inputs and atomic updates, Declared input actions, Documents and controls, Dynamic authored UI and input, Server authority and remaining scope

## Knowledge Gaps
- **1121 isolated node(s):** `version`, `joints`, `source_height`, `head_joint`, `source_forward` (+1116 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 4565 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **296 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Built-in capability parity audit` connect `Cross-cutting integration findings` to `modding/README.md`, `EntityAdapter`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **Why does `Audit boundary and live path` connect `Cross-cutting integration findings` to `ServerStartup`, `temp_save_dir`, `Registration`, `Replicas`?**
  _High betweenness centrality (0.029) - this node is a cross-community bridge._
- **Why does `Inventory` connect `VoxelView` to `PlayerDecision`, `invoke`, `EntityTransferPolicy`, `players/inventory.rs`, `Cache`, `server/durable.rs`, `Observations`, `notifications.rs`, `WorldSnapshot`, `server.rs`, `temp_save_dir`, `ClientApp`, `ClientMessage`, `Adapter`, `src/client.rs`, `crate`, `Stack`, `script_startup/gameplay.rs`, `join`, `prepare`, `Adapter`, `InventoryStore`, `Connection`, `CommitAction`, `model.rs`, `BlockEditCommand`, `InventoryWorkers`, `script_startup/gameplay/inventory.rs`?**
  _High betweenness centrality (0.025) - this node is a cross-community bridge._
- **What connects `version`, `joints`, `source_height` to the rest of the system?**
  _1121 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `EntityTypeDescriptor` be split into smaller, more focused modules?**
  _Cohesion score 0.08081632653061224 - nodes in this community are weakly interconnected._
- **Should `EntityStore` be split into smaller, more focused modules?**
  _Cohesion score 0.06515444015444015 - nodes in this community are weakly interconnected._
- **Should `super` be split into smaller, more focused modules?**
  _Cohesion score 0.01790633608815427 - nodes in this community are weakly interconnected._