# Graph Report - bloxgloom  (2026-09-30)

## Corpus Check
- 776 files · ~1,638,407 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 18 file(s) not represented in the graph (top: .wgsl 10, .ttf 5, (none) 1)

## Summary
- 12424 nodes · 30059 edges · 548 communities (396 shown, 152 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 1706 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `584812ce`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- EntityError
- EntityStore
- actions/entity.rs
- super
- preview.rs
- state_for
- Catalog
- world_to_chunk
- BlockActionContext
- OwnerData
- server/runtime/tests.rs
- Complete modding implementation proposal
- .advance
- OwnerPatch
- scheduler.rs
- server/durable.rs
- host-api/src/system.rs
- entities/types.rs
- OwnerKey
- Storage
- PackageSnapshot
- Cell
- protocol.rs
- Network
- OutboundFrame
- StateKey
- WorldSnapshot
- MobileProbe
- receipts.rs
- Inventory
- handler.rs
- Handler
- ui/draw.rs
- PendingWakeStore
- server_state_with_startup
- EntityIndexes
- EntityCodecError
- Inventories
- intent.rs
- SystemRuntime
- journal/rotation.rs
- join
- State
- actions/workstation.rs
- drops/planning.rs
- MobileEntity
- Renderer
- UiLayout
- server/fire/tests.rs
- VoxelView
- server/effects.rs
- protocol/tests.rs
- bench.rs
- intent/tests.rs
- ScriptError
- Replicas
- journal.rs
- ServerStartup
- script.rs
- Registrar
- client/entities/tests.rs
- server/drops/tests.rs
- parallel.rs
- perf/fixture.rs
- World
- .aimed_block
- host-api/src/machine.rs
- PalettedBlocks
- server/entities/tests.rs
- conflict_tests.rs
- GenerationError
- lifecycle-fixture/src/system.rs
- perf/fire.rs
- ChunkCache
- ClientMessage
- PublicEntity
- invoke_fields
- key
- reactor.rs
- config.rs
- .window_event
- src/world.rs
- InventoryStore
- Change
- TickSample
- tcp.rs
- Pending
- GameUi
- Port<P>
- owner/tests.rs
- Effect
- Receiver
- Authored materials and effects
- CheckpointWriter
- server/movement.rs
- EntityCheckpointMirror
- Connection
- ClientHandle
- TickId
- Attempt
- CommitAction
- serve
- package/client.rs
- server.rs
- JoinApp
- durable/state.rs
- RegisteredEffectError
- dispatch.rs
- UiFrame<'_>
- .spawn
- terrain.rs
- JobKey
- ComponentMatch
- ChunkLoader
- Journal
- script_startup/system.rs
- Session
- world/tests.rs
- content
- PlayerRules
- entities/checkpoint.rs
- entities
- burn.rs
- TerrainReads
- Registration
- HarvestSnapshot
- snapshots/tests.rs
- authored/tests.rs
- ScriptCreature
- diagnostics.rs
- net.rs
- ScriptSystem
- Plan: built-in/mod capability parity
- Payload
- InventoryId
- VisualSession
- entity_recovery/tests.rs
- effects/registered.rs
- host-api/src/actions.rs
- Appearance
- Committed
- presentation.rs
- render/mesh.rs
- Buffer
- Session
- public/tests.rs
- drops/entity.rs
- reaction_removal_tests.rs
- world/generation.rs
- publication/commit.rs
- .queue_command
- tests/anchored.rs
- HandlerRegistration
- InventoryScreen
- render/drops.rs
- Catalog
- Execution
- custom/shader.rs
- InventoryProbe
- PostProcess
- Decision
- reactor/codec.rs
- script_startup/creature.rs
- Growth foundation plan
- world/generation/tests.rs
- OwnerEffectPatch
- entity_checkpoint/tests.rs
- render/effects.rs
- plan
- view.rs
- drops/queries.rs
- Result
- TransportStats
- gameplay/decisions.rs
- Adapter
- handles.rs
- ui/tests.rs
- systems/world.rs
- Capture
- custom.rs
- init
- Scripting capabilities for mod developers
- Composition
- lighting.rs
- ClientApp
- .bind_machine
- UiRenderer
- AnchoredBlockEntity
- Declarations
- EntityDefinition
- snapshots.rs
- script_startup/gameplay.rs
- client/bundle.rs
- sealed_neighborhood
- Clock
- parallel/tests.rs
- Proposal: one coherent gameplay API
- Sender
- material.rs
- Adapter
- mlua
- Runtime
- declarer
- EntityTransferPolicy
- players/lifecycle.rs
- Update
- client/startup.rs
- VisualFire
- render/tests.rs
- machine_component_tests.rs
- register
- World
- process_movement_batch
- SystemDescriptor
- neighborhood.rs
- streaming.rs
- CoordinatorContext
- GpuPass
- StorageBlockEntity
- .compose_current_package_action_with_args
- wake.rs
- VisualAvatar
- Bindings
- ContentManifest
- Preparation
- prepare
- Stack
- Ignitions
- position_store.rs
- services.rs
- modding/README.md
- Context<'_>
- durable/checkpoint.rs
- inventory
- script_startup/appearance.rs
- pipeline.rs
- gameplay_anchor_tests.rs
- entity_sleep.rs
- EntityDependencies
- Execution foundation: next implementation slices
- EffectKindId
- script_startup.rs
- script_startup/bundle.rs
- FirePending
- integer
- Startup
- authored.rs
- src/composition.rs
- Patrol
- io
- client/lifecycle/tests.rs
- bundle_ui.rs
- Cross-cutting integration findings
- package/tests.rs
- install
- declarer
- slot.rs
- invoke
- .draw_node
- DroppedItem
- Resources
- Gpu
- PlayerSummary
- WorkstationView
- Hit
- system/intents.rs
- Input
- client/world.rs
- registry/tests.rs
- BootstrapContract
- Imports
- sync
- decode_transaction
- world_time/tests.rs
- package/manifest.rs
- Item
- server/appearance.rs
- arc
- .finish
- Scripting Gap Closure Plan
- ScriptMachine
- .public_view
- Invalid
- fs
- Wander
- Result
- owner_durable/tests.rs
- server/checkpoint/tests.rs
- Result
- route_registered_effects
- showcase.rs
- join_named_client
- ItemIcon
- stage_wave
- .begin_window_install
- Harvest
- declarer
- script_startup/machine.rs
- Capture
- request_chunk
- Inputs
- durable/fire/tests.rs
- admin/tests.rs
- EffectConsumerScratch
- Downloads
- receive_content_manifest
- SignalPost
- slots
- inventory/container.rs
- install
- world
- declarer
- server/drops.rs
- client_metadata.rs
- BundleIdentity
- raycast_blocks
- CodecProbe
- .register_action
- entities/container.rs
- script_startup/gameplay/entities.rs
- script_startup/gameplay/inventory.rs
- items.rs
- position_store/tests.rs
- chest.py
- complete_barrier
- FireAnimator
- declarer
- queries/tests.rs
- Player
- Replace
- 2. Player and lifecycle hooks
- invoke
- declarer
- script_startup/generation.rs
- tcp/report.rs
- Value
- streaming/entities.rs
- publication.rs
- src/actions/tests.rs
- Shared<T>
- lifecycle-fixture/src/machine.rs
- journal/recovery.rs
- player_services/tests.rs
- Bloxgloom interface plan
- DropPolicy
- materials.rs
- navigation.rs
- Resolved
- commands.rs
- script_startup/player.rs
- owner_wave/tests.rs
- create
- public_systems/tests.rs
- register
- Bloxgloom
- server/perf.rs
- drop_merge.rs
- .append_batch
- inventory/store.rs
- refund_stacks
- script_startup/machine/components.rs
- Registered inventory views and screens
- perf/report.rs
- .handle
- decode
- src/storage/tests.rs
- declarer
- .first_solid_top
- lifecycle-fixture/src/content.rs
- std
- generation
- coder.md
- .validate_player_selection
- coder-fast.md
- open
- gameplay/admin.rs
- config/tests.rs
- .new
- outbound/tests.rs
- decode
- spawn.rs
- Registered anchored behavior
- Atlas
- .handle
- fields_with_command
- EffectBuffer
- .plan
- render.rs
- seed
- resolve_player_movement
- avatars/tests.rs
- register
- MobilePages
- plan_observed_request
- decode
- Luau runtime tools
- validate_changes
- declarer
- JoinGuard
- .rotate_using
- .handle
- notifications.rs
- render/effects/tests.rs
- src/storage.rs
- .register
- Public dynamic-entity surface
- script/tests.rs
- .owner_systems
- Public storage lifecycle boundary
- combined/mixed.rs
- Registered content and composition
- Archived plans and audits
- drop_pickup.rs
- gameplay
- plan_changes
- host-api/src/content.rs
- presentation/effects/tests.rs
- Registered actions and composed controls
- client/appearance.rs
- client/drops/tests.rs
- predict_player_movement
- visual/tests.rs
- plan_motion
- run
- route_effects
- custom/tests.rs
- src/client/tests.rs
- join_lifecycle.rs
- EffectBuffer
- tests/effects.rs
- quad
- write_frame
- .prepare_benchmark_frontier_wave
- .generate
- Shared
- coder-smart.md
- Phase 8: examples, parity and integrated verification
- transfer/tests.rs
- run_loop
- Agent guidance
- .run
- ObserverRegistration
- visibility.rs
- EntityTypeId
- avatars/appearance.rs
- metadata
- actors/tests.rs
- create_target_pipeline
- falling/tests.rs
- SharedParts
- CheckpointWork
- Modding: start here
- .build
- reviewer.md
- net/tests.rs
- prepare_recovery
- stage
- .install_egui
- bloxgloom
- blocks/SOURCES.md
- foliage/SOURCES.md
- EGUI-POC.md
- verdant/assets/fonts/FONT.md
- fonts/README.md
- ui-entity-actions/packages/uitarget/assets/fonts/FONT.md
- ui-target-actions/packages/uitarget/assets/fonts/FONT.md
- Registered inventory machines
- crate
- src/client.rs
- palette/tests.rs
- .inventory_policies
- EntityTypeRegistryBuilder
- delivery.rs
- time.rs
- time
- PlayerState
- EffectLimits
- decode
- decode
- Chunk
- validate
- Diagnostics
- add_test_client
- kiln_footprint
- probe_request
- entity
- store/tests.rs
- profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default
- src/lifecycle/tests.rs
- Player lifecycle implementation
- publication/tests.rs
- declarer
- WorldTime
- avatars/mesh.rs
- voxel_view.rs
- SCRIPTING.md
- TEMP_ID
- saturating_wake_emitter
- validate_sources
- passes
- progress_rotation
- Entities, player behavior, world simulation and generation
- welcome/assets/fonts/FONT.md

## God Nodes (most connected - your core abstractions)
1. `EntityError` - 233 edges
2. `OwnerKey` - 140 edges
3. `SystemId` - 139 edges
4. `StateKey` - 116 edges
5. `TickId` - 103 edges
6. `ClientMessage` - 89 edges
7. `world_to_chunk()` - 85 edges
8. `CommitAction` - 82 edges
9. `temp_save_dir()` - 80 edges
10. `Value` - 78 edges

## Surprising Connections (you probably didn't know these)
- `F4 — New flammability meets a separate edit producer` --references--> `apply_synced_batch()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/server/durable/fire.rs
- `private_entity_overlay_does_not_authorize_the_next_decision_owner()` --calls--> `Probe`  [INFERRED]
  crates/host-api/src/gameplay/tests.rs → extensions/lifecycle-fixture/src/system.rs
- `Entities, player behavior, world simulation and generation` --references--> `parse()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/client/admin.rs
- `Entities, player behavior, world simulation and generation` --references--> `DropAnimator`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/client/drops.rs
- `Definitions, assets, and composition` --references--> `placeable_block_in()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/items.rs

## Import Cycles
- 2-file cycle: `src/server/script/runtime.rs -> src/server/script/runtime/diagnostics.rs -> src/server/script/runtime.rs`
- 2-file cycle: `src/server/perf/tcp.rs -> src/server/perf/tcp/report.rs -> src/server/perf/tcp.rs`
- 2-file cycle: `src/server/entities/spatial.rs -> src/server/entities/store.rs -> src/server/entities/spatial.rs`
- 2-file cycle: `src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server/block_actions.rs`
- 3-file cycle: `src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server/net.rs`
- 3-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server.rs`
- 3-file cycle: `src/server.rs -> src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server.rs`
- 3-file cycle: `src/server/parallel.rs -> src/server/parallel/owner_wave.rs -> src/server/registry.rs -> src/server/parallel.rs`
- 4-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server.rs`

## Communities (548 total, 152 thin omitted)

### Community 0 - "EntityError"
Cohesion: 0.05
Nodes (52): checked_body(), crc32(), Decoder, Decoder<'a>, Encoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION, ENTITY_CELL_VALUE_MAGIC (+44 more)

### Community 1 - "EntityStore"
Cohesion: 0.07
Nodes (36): validate_ownership_mode(), allocator_state_key(), apply_operation_to_projection(), canonical_location(), cell_state_key(), chunk_state_key(), collect_operations(), decode_entity_id() (+28 more)

### Community 2 - "actions/entity.rs"
Cohesion: 0.18
Nodes (18): capture_dependencies(), capture_entity_view_for_plan(), capture_tick_input(), capture_view_for_plan(), commit_tick_plan(), corrupt(), interaction_sight(), permission() (+10 more)

### Community 3 - "super"
Cohesion: 0.02
Nodes (18): KEY, State, abandoned_result_retires_transport(), completed_result_can_still_be_cancelled(), failure_and_retry_dispatch(), CYCLE_MS, INITIAL_MS, Codec (+10 more)

### Community 4 - "preview.rs"
Cohesion: 0.05
Nodes (51): render_block_preview(), CLIENT_MESH_RESULT_BATCH, CLIENT_PENDING_UPLOADS, render_daylight_previews(), DropPhase, read_rgba_png(), render(), render_egui_previews() (+43 more)

### Community 5 - "state_for"
Cohesion: 0.12
Nodes (36): drain_durable(), drop_active_len(), drop_nearby(), reside_neighbourhood(), spawn_drop(), state_for(), drop_totals(), listener_drop_motion_merge_pickup_and_restart_use_one_durable_path() (+28 more)

### Community 6 - "Catalog"
Cohesion: 0.04
Nodes (54): ACTIVE, block_flags(), BlockDef, BlockTextures, BUILTIN_EMISSION, BUILTIN_FLAGS, BUILTIN_REFLECTANCE, Catalog (+46 more)

### Community 7 - "world_to_chunk"
Cohesion: 0.09
Nodes (67): unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers(), hopper_push_is_atomic_conflict_checked_and_resumes_after_full_destination(), registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes(), public_spawn_and_self_removal_are_one_atomic_recoverable_transaction(), plan_durable_request(), action_without_an_authoritative_handler_fails_before_world_creation(), register_probe(), registered_block_observation_is_required_and_remains_fenced_at_admission() (+59 more)

### Community 8 - "BlockActionContext"
Cohesion: 0.06
Nodes (22): BlockActionContext, BlockActionHooks, BlockActionRegistry, BlockActionRegistryBuilder, BlockActionRegistryBuilder<'a>, BlockCommitBuilder, invoke_hook(), MAX_BLOCK_ACTION_HANDLERS (+14 more)

### Community 9 - "OwnerData"
Cohesion: 0.05
Nodes (15): OwnerCodec, OwnerData, Codec, config(), prepare(), OwnerCodecError, OwnerValueCodec, BytesCodec (+7 more)

### Community 10 - "server/runtime/tests.rs"
Cohesion: 0.06
Nodes (46): captured_owner_reads_share_reservations_and_fence_exclusive_waves(), rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor(), due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility(), increment(), invalid_handler_deadline_rejects_without_losing_due_work(), mixed_active_and_recurring_due_owners_progress_with_and_without_wakes(), multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation(), run() (+38 more)

### Community 11 - "Complete modding implementation proposal"
Cohesion: 0.05
Nodes (39): 10. Custom models: explicitly deferred, 11. Developer workflow and maintenance rules, 12. Implementation order and deliverables, 13. Completion and verification, 14. Continuation record — update during implementation, 1. What approval means, 2. Product outcome, 3. Starting point: preserve the useful work (+31 more)

### Community 12 - ".advance"
Cohesion: 0.35
Nodes (4): Body, DT, Movement, valid_position()

### Community 13 - "OwnerPatch"
Cohesion: 0.05
Nodes (16): MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_PATCH_BYTES_PER_JOB, MAX_OWNER_PATCH_WRITES_PER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, MAX_OWNER_WAVE_PATCH_WRITES, OwnerJob, OwnerJobError, OwnerPatch (+8 more)

### Community 14 - "scheduler.rs"
Cohesion: 0.07
Nodes (21): cursor_lane(), FireRuntime, source_transaction(), FIRE_DELIVERY_SYSTEM_ID, FIRE_LANES, FIRE_SYSTEM_ID, FireCursor, FireLoadMetrics (+13 more)

### Community 15 - "server/durable.rs"
Cohesion: 0.04
Nodes (18): stage(), Durability, BlockDelta, CHECKPOINT_QUEUE_CAPACITY, CHECKPOINT_WORKERS, DirtyCheckpoint, Durability, FireCheckpointBatch (+10 more)

### Community 16 - "host-api/src/system.rs"
Cohesion: 0.07
Nodes (20): Behavior, Context, DropSpawn, EntityChange, EntitySpawn, IntentDelivery, IntentId, IntentOutbox (+12 more)

### Community 17 - "entities/types.rs"
Cohesion: 0.04
Nodes (31): MAX_PLAYER_ENTITY_PAYLOAD_BYTES, MAX_SESSION_PLAYER_ENTITIES, PLAYER_ENTITY_TYPE, player_public_payload_codec_is_fixed_size(), player_type_registers_only_against_catalogued_identity(), PlayerEntityPayload, PlayerEntityStore, PlayerPayloadCodec (+23 more)

### Community 18 - "OwnerKey"
Cohesion: 0.04
Nodes (45): ChunkKey, OwnerKey, SystemId, crc32(), decode_cell_value(), decode_cursor_value(), decode_owner_cursor_key(), decode_owner_state_key() (+37 more)

### Community 19 - "Storage"
Cohesion: 0.25
Nodes (3): checksum(), SavedEdits, Storage

### Community 20 - "PackageSnapshot"
Cohesion: 0.05
Nodes (14): authored_drop_animation_negotiates_and_default_keeps_old_bundle(), drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle(), item_sprite_option_is_negotiated_and_omission_preserves_default_identity(), handler_declarer(), error(), MAX_ASSET_BYTES, MAX_ASSETS, MAX_MANIFEST_BYTES (+6 more)

### Community 21 - "Cell"
Cohesion: 0.08
Nodes (9): Cell, Block, cell_random(), Context, Context<'a>, DropSpawn, Error, Plan (+1 more)

### Community 22 - "protocol.rs"
Cohesion: 0.10
Nodes (30): BLOCK_COUNT, Cursor, Cursor<'a>, frame(), invalid(), key(), MAX_ENTITY_INTERACT_BYTES, MAX_FIRE_BURSTS (+22 more)

### Community 23 - "Network"
Cohesion: 0.08
Nodes (13): ConfigWriter, connect_bundle_probe(), connect_catalog_probe(), connect_inventory_probe(), connect_ui_probe(), connect_visual_probe(), Incoming, Mesher (+5 more)

### Community 24 - "OutboundFrame"
Cohesion: 0.10
Nodes (11): ClientQueueTelemetry, OUTBOUND_AGGREGATE_BYTE_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundError, OutboundFrame, OutboundQueue, OutboundTelemetry (+3 more)

### Community 25 - "StateKey"
Cohesion: 0.11
Nodes (23): decode_snapshot(), encode_snapshot(), FILE_NAME, MAGIC, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_KEYS, read_snapshot_file(), TEMP_SEQUENCE (+15 more)

### Community 26 - "WorldSnapshot"
Cohesion: 0.09
Nodes (10): block(), combine_entities(), dispatch_neighbors(), error(), OperationInput, Participants, plan_removals(), plan_with_lifecycles() (+2 more)

### Community 27 - "MobileProbe"
Cohesion: 0.06
Nodes (10): MobileProbe, NetworkedVisualProbe, console_commands_reject_non_admin_and_recover_grant_over_nonblocking_listener(), creature_probe(), external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener(), mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart(), mixed_work(), response_samples() (+2 more)

### Community 28 - "receipts.rs"
Cohesion: 0.09
Nodes (22): Admission, checksum(), invalid(), MAGIC, MAX_PAYLOAD, MAX_REASON, MAX_SNAPSHOT, ReceiptEvent (+14 more)

### Community 29 - "Inventory"
Cohesion: 0.04
Nodes (40): Inventory, FUEL_SLOT_INDEX, fuel_ticks(), INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS, KILN_MAX_FUEL_TICKS, KILN_MAX_PAYLOAD_BYTES, KILN_MAX_RECIPES (+32 more)

### Community 30 - "handler.rs"
Cohesion: 0.07
Nodes (15): FireFrontier, FireDeliveryHandler, FireDeliveryInput, FireDeliveryPatch, FireHandler, FireOwnerInput, FireOwnerPatch, MAX_DELIVERIES_PER_OWNER (+7 more)

### Community 31 - "Handler"
Cohesion: 0.09
Nodes (19): Handler, Destroy, GroundRemoved, Neighbor, Removed, ChestBreak, CollectDrop, FlowerNeighbor (+11 more)

### Community 32 - "ui/draw.rs"
Cohesion: 0.10
Nodes (23): UiBuilder<'_>, UiBuilder<'_>, EDGE, FONT_HEIGHT, FONT_WIDTH, GOLD, inset(), item_color() (+15 more)

### Community 33 - "PendingWakeStore"
Cohesion: 0.09
Nodes (24): crc32(), decode_owner_wake_key(), decode_wake_value(), encode_wake_value(), invalid_data(), OWNER_WAKE_DOMAIN, owner_wake_key(), OWNER_WAKE_MAGIC (+16 more)

### Community 34 - "server_state_with_startup"
Cohesion: 0.12
Nodes (39): tick_once(), server_state_with_startup(), durable_counter_startup(), durable_pair_startup(), durable_twin_startup(), entity_and_owner_state_commit_as_one_atomic_record(), external_neighbor_reads_defer_until_all_chunks_arrive_and_fence_adjacent_edits(), external_owner_can_durably_wake_another_owner_after_restart() (+31 more)

### Community 35 - "EntityIndexes"
Cohesion: 0.11
Nodes (15): _entity_cell_key_round_trip(), _entity_page_key_round_trip(), Bucket, ChunkPage, decode_cell_key(), decode_cell_owner(), decode_chunk_key(), encode_cell_key() (+7 more)

### Community 36 - "EntityCodecError"
Cohesion: 0.06
Nodes (15): BinCodec, CounterCodec, MateCodec, WideCodec, ByteCodec, Codec, decode_stack(), encode_payload() (+7 more)

### Community 37 - "Inventories"
Cohesion: 0.10
Nodes (6): handler_random_is_stable_per_seed_cell_and_registration(), ignored_failures_cannot_publish_partial_operations(), Inventories, private_entity_overlay_does_not_authorize_the_next_decision_owner(), transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides(), World

### Community 38 - "intent.rs"
Cohesion: 0.10
Nodes (18): blocked(), decode(), decode_key(), encode(), is_key(), key(), Reader, Reader<'a> (+10 more)

### Community 39 - "SystemRuntime"
Cohesion: 0.08
Nodes (7): MAX_OWNER_VALUES_PER_SYSTEM, MAX_PENDING_OWNER_WAKES, PendingRegisteredWave, PreparedRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs, SystemRuntime

### Community 40 - "journal/rotation.rs"
Cohesion: 0.15
Nodes (36): invalid_data(), Base, BASE_FORMAT_VERSION, BASE_FORMAT_VERSION_LEGACY, BASE_MAGIC, BASE_MAX_BYTES, base_path(), check_checksum() (+28 more)

### Community 41 - "join"
Cohesion: 0.08
Nodes (36): failed_startup_queue_does_not_register_a_ghost_profile(), full_outbound_queue_disconnects_only_the_slow_client(), joined_players_spawn_above_solid_terrain_with_headroom(), joins_find_lower_safe_surface_after_origin_support_is_mined(), multiple_clients_receive_edit_delta_then_resync_snapshot_in_order(), remote_player_spawn_move_and_leave_publish_ordered_entity_changes(), startup_and_live_spawn_can_use_negative_ground_after_excavation(), streamed_interest_pins_release_on_resync_and_disconnect() (+28 more)

### Community 42 - "State"
Cohesion: 0.09
Nodes (8): Client, handle_message(), package_bundle_for_preview(), queue_move(), server_state_with_limit(), server_state_with_limit_and_catalog(), State, independent_worlds_keep_their_own_startup_catalogs()

### Community 43 - "actions/workstation.rs"
Cohesion: 0.08
Nodes (16): block_intersects_player(), plan(), plan(), validate_player_credit(), is_registered(), plan(), cached_block_or_request(), cached_block_with_reads() (+8 more)

### Community 44 - "drops/planning.rs"
Cohesion: 0.12
Nodes (25): merge_target(), plan_error(), plan_expired(), plan_spawn_stack(), plan_spawns(), plan_spawns_with_extra(), plan_stack_spawns(), plan_stack_spawns_with_extra() (+17 more)

### Community 45 - "MobileEntity"
Cohesion: 0.10
Nodes (16): Animation, Behavior, Body, Context, Cuboid, Error, Lifecycle, MobileEntity (+8 more)

### Community 46 - "Renderer"
Cohesion: 0.05
Nodes (7): next_upload_index(), order_pending_mesh(), Renderer, RendererError, RenderStats, urgent_mesh_reorders_existing_pending_chunk_without_duplication(), create_depth()

### Community 47 - "UiLayout"
Cohesion: 0.11
Nodes (9): InventorySearch, search_rect(), join_action_rect(), centered_panel(), effective_ui_scale(), HitRect, UiLayout, UiControl (+1 more)

### Community 48 - "server/fire/tests.rs"
Cohesion: 0.12
Nodes (32): cursor_key(), frontier_key(), benchmark_frontier_bootstrap_precedes_first_live_fire_tick(), checkpoint_batch_replaces_one_complete_snapshot_and_applies_tombstones(), checkpoint_store_rejects_orphans_and_corruption_and_cleans_interrupted_temp(), chunk(), durable_lane_age_prioritizes_an_owner_deferred_by_wal_pressure(), first_aggregate_write_preserves_legacy_per_key_checkpoints() (+24 more)

### Community 49 - "VoxelView"
Cohesion: 0.04
Nodes (26): CapturedColumn, DropTickPlanner, spawn_effects(), AnchorEchoTick, BadTick, BinExchange, CounterInteract, CounterTick (+18 more)

### Community 50 - "server/effects.rs"
Cohesion: 0.19
Nodes (12): block_change_owners(), boundary_coordinates(), CellCoord, Effect, EffectBatch, MAX_EFFECTS_PER_BATCH, MAX_EFFECTS_PER_OWNER, MAX_EFFECTS_PER_PRODUCER_TICK (+4 more)

### Community 51 - "protocol/tests.rs"
Cohesion: 0.13
Nodes (25): read_server(), action_receipts_round_trip_and_reject_invalid_ids(), admin_grant_wire_round_trips_and_rejects_invalid_counts(), catalog_with_many_states(), client_messages_round_trip(), committed_fire_cues_round_trip_with_a_strict_cell_bound(), content_parts_and_ready_round_trip_with_bounds(), entity_interaction_rejects_empty_oversized_and_truncated_payloads() (+17 more)

### Community 52 - "bench.rs"
Cohesion: 0.07
Nodes (18): FireApplyTimings, FireRuntime, ACTIVE_CHUNKS, benchmark_cpu(), BenchSave, cell_index(), CHUNKS_X, CHUNKS_Z (+10 more)

### Community 53 - "intent/tests.rs"
Cohesion: 0.11
Nodes (32): durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans(), durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation(), durable_intent_bootstrap_destination_conflict_retries_one_atomic_record(), durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required(), durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems(), cell(), chunk(), durable_intent_bootstrap_chain_gates_creation_retry_forwarding_and_restart() (+24 more)

### Community 54 - "ScriptError"
Cohesion: 0.10
Nodes (14): CacheKey, command_metadata_roundtrips_schema_and_permissions_and_rejects_forgery(), invalid(), decode(), MAGIC, wrap(), ordered(), decode() (+6 more)

### Community 55 - "Replicas"
Cohesion: 0.08
Nodes (15): Interaction, presentation and durability closure, Assembly, kiln_adapter(), MAX_CHUNK_ENTITY_BYTES, MAX_CLIENT_ENTITY_BYTES, MAX_PENDING_BYTES, MAX_PENDING_COMMITS, MAX_PENDING_SNAPSHOTS (+7 more)

### Community 56 - "journal.rs"
Cohesion: 0.08
Nodes (17): CLOCK_DOMAIN, clock_key(), FILE_HEADER_LEN, FILE_MAGIC, FILE_VERSION, FRAME_OVERHEAD, JOURNAL_ROTATION_SOFT_LIMIT_BYTES, MAX_BATCH_RECORDS (+9 more)

### Community 57 - "ServerStartup"
Cohesion: 0.10
Nodes (5): entity_error(), ServerStartup, ServerStartup, StartupEntityType, StartupOwnerCodec

### Community 58 - "script.rs"
Cohesion: 0.10
Nodes (13): Invocation, LimitExceeded, Limits, Output, Program, Request, run(), run_presentation() (+5 more)

### Community 59 - "Registrar"
Cohesion: 0.10
Nodes (6): CubeBlock, Registrar, RegistrationError, Fixture, KEY, TallStore

### Community 60 - "client/entities/tests.rs"
Cohesion: 0.10
Nodes (30): anchored_presentation_window_is_separate_scoped_and_sorted(), mossbun_adapter_uses_the_negotiated_catalog_assignment(), mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction(), presentation_entity_window_is_ordered_and_explicitly_bounded(), accept(), block_and_entity_changes_wait_for_whole_cross_chunk_commit(), checksum_conflict_and_revision_gap_request_resync_without_partial_install(), chunk() (+22 more)

### Community 61 - "server/drops/tests.rs"
Cohesion: 0.11
Nodes (30): active_len(), apply_expired(), assert_store_consistent(), drop_world(), drop_world_in(), DropWorld, insert_entry(), item() (+22 more)

### Community 62 - "parallel.rs"
Cohesion: 0.11
Nodes (21): BarrierError, CancellationToken, execute_task(), ExecutorConfigError, JobCompletion, JobOutcome, MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY (+13 more)

### Community 63 - "perf/fixture.rs"
Cohesion: 0.09
Nodes (23): ACTION_INTERVAL, add_clients_and_seed_drops(), DIRT_ITEM, drain_outbound(), DrainTotals, DROP_HEIGHTS, install_chunks(), MAX_STEADY_TICKS (+15 more)

### Community 64 - "World"
Cohesion: 0.10
Nodes (6): ChunkKey, ChunkReadStamp, EditBasis, LoadedChunk, PreparedEdit, World

### Community 65 - ".aimed_block"
Cohesion: 0.14
Nodes (6): action_id(), command_action_id(), digit_slot(), edit_for_hit(), edit_for_hit_with_catalog(), cardinal_placement_hint_follows_camera_yaw()

### Community 67 - "host-api/src/machine.rs"
Cohesion: 0.05
Nodes (24): Behavior, Context, DownwardFlow, FACES, Filter, Fuel, Machine, Plan (+16 more)

### Community 68 - "PalettedBlocks"
Cohesion: 0.10
Nodes (6): LocalIndex, PalettedBlocks, PaletteView, set_palette_cell(), u16, u8

### Community 69 - "server/entities/tests.rs"
Cohesion: 0.13
Nodes (22): encode_checkpoint(), write_checkpoint(), checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types(), delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion(), fixture_registry(), frozen_wake_registry(), gameplay_entity_reads_fence_absence_without_fencing_unrelated_mobile_occupancy(), mixed_entity_batch_coalesces_updates_transfer_despawn_and_spawn_atomically() (+14 more)

### Community 70 - "conflict_tests.rs"
Cohesion: 0.12
Nodes (25): action(), coordinator_admits_two_independent_atomic_pickups_before_applying_either(), disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply(), drop_merge_absence_is_fenced_against_same_owner_motion_into_range(), hold(), neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders(), overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership(), plan() (+17 more)

### Community 71 - "GenerationError"
Cohesion: 0.10
Nodes (11): CHUNK_SIZE, Context, Contributor, GenerationError, in_world_bounds(), MAX_WRITES, mix(), Output (+3 more)

### Community 72 - "lifecycle-fixture/src/system.rs"
Cohesion: 0.13
Nodes (12): Clock, definition(), KEY, NeighborProbe, Pair, pair_definition(), Probe, WakeLoop (+4 more)

### Community 73 - "perf/fire.rs"
Cohesion: 0.11
Nodes (22): ACTIVE_CHUNKS, CHUNKS_X, CHUNKS_Z, drain_durable(), ensure_resident(), fixture_action(), FOREST_BATCH_CHUNKS, forest_hash() (+14 more)

### Community 74 - "ChunkCache"
Cohesion: 0.12
Nodes (3): CacheEntry, ChunkCache, OwnerState

### Community 75 - "ClientMessage"
Cohesion: 0.07
Nodes (3): PackageActionProbe, PackageActionProbe, ClientMessage

### Community 76 - "PublicEntity"
Cohesion: 0.11
Nodes (28): Replicas, BlockCellChange, enforce_frame_size(), EntitySnapshotPage, MAX_BLOCK_CHANGES_PER_PART, MAX_ENTITIES_PER_PAGE, MAX_ENTITY_CHANGES_PER_PART, MAX_ENTITY_SNAPSHOT_BYTES (+20 more)

### Community 77 - "invoke_fields"
Cohesion: 0.13
Nodes (11): cell_at(), checked(), entity_id(), invalid(), invoke_fields(), position_at(), install(), latch() (+3 more)

### Community 78 - "key"
Cohesion: 0.29
Nodes (13): animated_item_bundle_rejects_invalid_and_noncanonical_motion(), canonical_order_dependency_identity_and_count_bounds_are_verified(), decoder_rejects_server_classification_and_oversized_payloads_before_copying(), header(), key(), material_effect_and_ui_assets_coexist_in_canonical_bundle(), metadata_validates_namespace_capability_shape_and_limits_before_compilation(), package() (+5 more)

### Community 79 - "reactor.rs"
Cohesion: 0.10
Nodes (19): ACCEPT_BUDGET, has_admission_capacity(), has_admission_capacity_with_limit(), inventory_worker(), INVENTORY_WORKERS, InventoryLoadRequest, InventoryWorkers, IO_POLL_TIMEOUT (+11 more)

### Community 80 - "config.rs"
Cohesion: 0.10
Nodes (17): clamp_finite(), Config, CONFIG_VERSION, create_temporary_file(), MAX_FOV, MAX_SCALE, MAX_SENSITIVITY, MIN_FOV (+9 more)

### Community 81 - ".window_event"
Cohesion: 0.12
Nodes (4): escape_screen(), inventory_screen(), ClientApp, SettingId

### Community 82 - "src/world.rs"
Cohesion: 0.09
Nodes (31): AIR, BEDROCK_Y, BLUE_FLOWER, CHUNK_SIZE, CHUNK_VOLUME, DIRT, FERN, GLOWSTONE (+23 more)

### Community 83 - "InventoryStore"
Cohesion: 0.24
Nodes (3): checksum(), invalid(), InventoryStore

### Community 84 - "Change"
Cohesion: 0.08
Nodes (18): Change, arbitrate_key_sets(), build_owner_writes_parallel(), canonical_key_set(), OwnerCommit, OwnerWaveDurables, OwnerWorldAction, canonical_key_sets_collapse_duplicates_and_sort() (+10 more)

### Community 85 - "TickSample"
Cohesion: 0.11
Nodes (13): duration_nanos(), EVENT_LATENCY_STREAMS, LatencyEvent, LatencyRing, Metric, MetricsRecorder, nearest_rank(), PHASE_COUNT (+5 more)

### Community 86 - "tcp.rs"
Cohesion: 0.11
Nodes (15): drive(), exercise(), movement(), open_nuisance_peers(), PROFILE_BASE, run(), SEED, seed_inventories() (+7 more)

### Community 87 - "Pending"
Cohesion: 0.08
Nodes (8): Declarations, invoke(), MAX_BLOCKS_PER_PACKAGE, MAX_ITEMS_PER_PACKAGE, MAX_TEXTURES_PER_PACKAGE, PackageTexture, Pending, text()

### Community 88 - "GameUi"
Cohesion: 0.08
Nodes (5): draw_screen(), DrawTarget, GameUi, Intent, themed_context()

### Community 89 - "Port<P>"
Cohesion: 0.13
Nodes (6): block(), Interaction, Interaction<P>, Port, Port<P>, public_slots()

### Community 90 - "owner/tests.rs"
Cohesion: 0.11
Nodes (19): Behavior, CELL, drop_count(), empty_action(), Fixture, Harvest, KEY, Marker (+11 more)

### Community 91 - "Effect"
Cohesion: 0.12
Nodes (3): Effect, target_sizes(), targets_remain_bounded_at_large_and_tiny_viewports()

### Community 92 - "Receiver"
Cohesion: 0.09
Nodes (8): Receiver, CommitReceipt, Journal, JournalWriter, Request, RotateError, RotationReceipt, WriterCommand

### Community 93 - "Authored materials and effects"
Cohesion: 0.40
Nodes (5): Authored materials and effects, Effect contract 2, Material contract 2, Preparation and compatibility, Typed values from Luau

### Community 94 - "CheckpointWriter"
Cohesion: 0.12
Nodes (7): checkpoint_shard(), checkpoint_worker(), CheckpointJob, CheckpointReceipt, CheckpointSubmitError, CheckpointWriter, panic_message()

### Community 95 - "server/movement.rs"
Cohesion: 0.12
Nodes (13): AckKind, credit_per_tick(), CREDIT_SCALE, FLOAT_ROUNDING_ALLOWANCE_PER_TICK, MAX_COMMANDS_PER_TICK, max_credit(), movement_cost(), MovementAck (+5 more)

### Community 96 - "EntityCheckpointMirror"
Cohesion: 0.14
Nodes (10): EntityMotionSnapshot, CheckpointReceipt, CheckpointTicket, Command, EntityCheckpointMirror, Event, MAX_MIRROR_ADMISSIONS, MirrorMetrics (+2 more)

### Community 97 - "Connection"
Cohesion: 0.17
Nodes (6): Connection, PendingWrite, PendingWriteKind, Phase, PendingLeave, SimulationInput

### Community 98 - "ClientHandle"
Cohesion: 0.14
Nodes (9): ActionKind, active_slow_peer(), ClientHandle, ClientStats, handshake(), PendingAction, read_until_stop(), Ready (+1 more)

### Community 99 - "TickId"
Cohesion: 0.13
Nodes (13): ClockError, CommandQueue, CommandQueue<T>, DrainError, FIXED_STEP, FixedStepClock, OrderedCommand, OrderKey (+5 more)

### Community 100 - "Attempt"
Cohesion: 0.12
Nodes (5): Attempt, Control, Prepared, Progress, JoinProgress

### Community 101 - "CommitAction"
Cohesion: 0.11
Nodes (29): CommitAction, batchable_motion(), cancel_prepared_entities(), command_action_id(), defer_action(), durable_request_profile(), fail_if_durability_failed(), fatal_stage_error() (+21 more)

### Community 102 - "serve"
Cohesion: 0.05
Nodes (41): luau_action_block_targets_keep_real_reach_sight_and_identity_checks(), luau_creature_replaces_itself_with_another_authored_type_over_real_listener(), mod_admin_grant_requires_server_identity_and_replays_once_over_listener(), mod_admin_spawn_and_drop_share_one_allocator_and_restart(), connect(), package(), package_cube_flags_are_frozen_and_old_declaration_keeps_defaults(), package_cube_joins_places_and_recovers_with_identical_session_catalog() (+33 more)

### Community 103 - "package/client.rs"
Cohesion: 0.06
Nodes (34): ANIMATED_MAGIC, APPEARANCE_MAGIC, APPEARANCE_POLICY_MAGIC, BLOCK_OPTIONS_MAGIC, BLOCK_STATES_MAGIC, ClientBundle, ClientPackage, ClientSide (+26 more)

### Community 104 - "server.rs"
Cohesion: 0.09
Nodes (11): DEFAULT_CLIENTS, DEFAULT_VIEW, EDIT_REACH, INPUT_CAPACITY, LOADER_CAPACITY, MAX_CATCH_UP_TICKS, MAX_CLIENTS, MAX_INPUTS_PER_TICK (+3 more)

### Community 106 - "durable/state.rs"
Cohesion: 0.25
Nodes (9): action_changes(), chunk_state_key(), decode_chunk_key(), decode_profile_key(), encode_action_receipt(), encode_action_receipt_with_catalog(), invalid_data(), inventory_state_key() (+1 more)

### Community 107 - "RegisteredEffectError"
Cohesion: 0.21
Nodes (4): EffectConsumerOutput, ErasedEffectKind, RegisteredEffectError, TypedEffectKind<P, M, V, D, C>

### Community 108 - "dispatch.rs"
Cohesion: 0.14
Nodes (15): apply(), disconnect(), prepare(), Prepared, publish(), effect(), fire_cue_is_dropped_for_backlogged_client_without_disconnect(), fire_cue_requires_subscription_and_follows_committed_world_frame() (+7 more)

### Community 109 - "UiFrame<'_>"
Cohesion: 0.11
Nodes (17): draw(), actions(), admin(), button(), draw(), EDGE, GOLD, draw() (+9 more)

### Community 110 - ".spawn"
Cohesion: 0.15
Nodes (4): Lane, Mailbox, Runner, Snapshot

### Community 111 - "terrain.rs"
Cohesion: 0.17
Nodes (28): Biome, collapse_surface(), Column, decorate_chunk(), generate_blocks(), generated_block(), generated_block_in_column(), generated_block_with_pattern() (+20 more)

### Community 112 - "JobKey"
Cohesion: 0.10
Nodes (6): BatchId, CancelError, JobKey, PhaseExecutor<R, E>, SubmitError, F

### Community 113 - "ComponentMatch"
Cohesion: 0.15
Nodes (13): ComponentMatch, ComponentOutput, ComponentValue, value_bytes(), decode_exact(), decode_input(), decode_output(), encode_exact() (+5 more)

### Community 114 - "ChunkLoader"
Cohesion: 0.08
Nodes (16): ChunkLoader, ChunkLoadResult, ChunkLoadTicket, Job, RequestError, RequestStatus, stop_workers(), accepted_work_budget_has_explicit_nonblocking_overflow() (+8 more)

### Community 115 - "Journal"
Cohesion: 0.11
Nodes (3): Journal, KnownRecord, Transaction

### Community 116 - "script_startup/system.rs"
Cohesion: 0.15
Nodes (21): commit(), Fixture, KEY, luau_burn_owner_uses_host_removal_semantics_and_persists_receipt(), luau_owner_after_dependencies_are_resolved_before_save_creation(), luau_owner_block_info_uses_captured_public_fields_and_restarts(), luau_owner_caught_invalid_entity_change_rejects_whole_wave(), luau_owner_drop_creation_shares_receipt_and_restarts() (+13 more)

### Community 118 - "world/tests.rs"
Cohesion: 0.10
Nodes (26): biomes_cover_distinct_surfaces_across_an_endless_world(), broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline(), chunk_cache_evicts_the_least_recently_used_resident(), collapsed_surface_obeys_constraints_and_matches_region_edges(), composed_generation_is_ordered_and_does_not_change_builtin_baseline(), composed_generation_rejects_unknown_states_and_duplicate_keys(), corrupt_save_is_not_silently_discarded(), edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot() (+18 more)

### Community 119 - "content"
Cohesion: 0.11
Nodes (18): drop_snapshot(), dropped_column_lands_at_rest_and_suspends(), falling_drop_integrates_exactly_one_fixed_step(), missing_terrain_defers_fail_closed_without_guessing(), neighbourhood_view(), neighbours(), NEXT_TEST_DIR, planner_rejects_anchored_locations_and_foreign_payloads() (+10 more)

### Community 120 - "PlayerRules"
Cohesion: 0.11
Nodes (9): Body, BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_RULES, BUILTIN_SPAWN, InvalidPlayerRules, MotionRates, PlayerRules (+1 more)

### Community 121 - "entities/checkpoint.rs"
Cohesion: 0.13
Nodes (13): CHECKPOINT_NAME, checkpoint_rejects_bad_magic_version_and_checksum(), checkpoint_write_read_is_atomic_and_bounded(), crash_left_temporary_checkpoint_fails_closed(), DIRECTORY_NAME, entity_invalid_data(), EntityCheckpointStore, invalid_data() (+5 more)

### Community 122 - "entities"
Cohesion: 0.31
Nodes (5): live_command(), missing_mossbun_terrain_defers_locally_then_runs_on_residency(), mossbun_authorized_spawn_worker_steps_and_restart_preserve_identity(), mossbun_spawn_limit_rejects_without_allocating_or_consuming_items(), resident_platform()

### Community 123 - "burn.rs"
Cohesion: 0.11
Nodes (11): assert_burned(), assert_uncommitted(), Burn, burn_startup(), BurnGrass, definition(), destination(), plant_burn() (+3 more)

### Community 124 - "TerrainReads"
Cohesion: 0.15
Nodes (8): TerrainReads, anchored(), nearby(), project(), read(), state(), validate_owner(), validate_state()

### Community 126 - "HarvestSnapshot"
Cohesion: 0.14
Nodes (4): flower_harvests_itself_and_grass_and_leaves_have_distinct_loot(), harvest(), harvest_with_catalog(), HarvestSnapshot

### Community 127 - "snapshots/tests.rs"
Cohesion: 0.15
Nodes (17): captured_revision_is_rejected_after_confirmed_edit_and_recaptured_in_order(), changed_interest_or_reconnected_session_cannot_receive_pending_capture(), dense_snapshot_disconnects_only_affected_client_and_closes_earlier_jobs(), differing_epochs_do_not_share_wire_content(), distinct_chunk_capture_is_bounded_and_rotates_to_deferred_clients(), Fixture, live_stream_prepares_once_and_shares_encoded_pages_for_matching_clients(), pressure_reclaim_is_worker_prepared_and_coordinator_announces_then_releases() (+9 more)

### Community 128 - "authored/tests.rs"
Cohesion: 0.14
Nodes (21): action_callback_cannot_supply_target_or_authorization_claims(), change_json(), decode(), dynamic(), egui_input_dispatches_unicode_text_and_preserves_busy_value(), encode(), encode_source(), handlers_fail_closed_atomically_with_module_attribution_and_sandbox_limits() (+13 more)

### Community 129 - "ScriptCreature"
Cohesion: 0.16
Nodes (3): invalid(), ScriptCreature, State

### Community 130 - "diagnostics.rs"
Cohesion: 0.16
Nodes (11): Buffer, encode_fields(), full(), invalid(), MAX_BYTES, MAX_MESSAGE, MAX_RECORDS, primitive() (+3 more)

### Community 131 - "net.rs"
Cohesion: 0.10
Nodes (16): BUNDLE_TIMEOUT, ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT, JoinCleanup, bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline(), commands_received_after_content_ready_wait_for_join_completion(), connected_streams() (+8 more)

### Community 132 - "ScriptSystem"
Cohesion: 0.12
Nodes (6): bytes(), cell(), declarer(), field(), ScriptSystem, table()

### Community 133 - "Plan: built-in/mod capability parity"
Cohesion: 0.08
Nodes (24): 1. Establish the boundary and parity inventory, 2. Complete the container/block-entity vertical slice, 3. Complete dynamic entities and presentation, 4. Close remaining gameplay and world surfaces, 5. Prove integration outside engine internals, Adopted direction, Client presentation and resources, Completed task: player-response path hardening (+16 more)

### Community 134 - "Payload"
Cohesion: 0.12
Nodes (6): Behavior, Context, interaction_request(), Reaction, RemovalCause, Payload

### Community 135 - "InventoryId"
Cohesion: 0.20
Nodes (7): Components, Context<'_>, InventoryId, PickupTransfer, PickupTransfer<'a>, Slot, Stack

### Community 137 - "entity_recovery/tests.rs"
Cohesion: 0.17
Nodes (12): checkpointed_motion_ahead_of_wal_fence_survives_recovery(), fixture(), lagging_checkpoint_replays_later_wal_transfer(), MOBILE_TYPE, NEXT_DIR, one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt(), position(), same_revision_conflicting_checkpoint_motion_fails_closed() (+4 more)

### Community 138 - "effects/registered.rs"
Cohesion: 0.13
Nodes (13): MAX_EFFECT_BATCH_PAYLOAD_BYTES, MAX_EFFECT_BUFFER_PAYLOAD_BYTES, MAX_EFFECT_DESTINATIONS, MAX_EFFECT_KIND_ID_BYTES, MAX_EFFECT_KIND_PAYLOAD_BYTES, MAX_REGISTERED_EFFECT_KINDS, RegisteredEffectBuffer, RegisteredEffectIntent (+5 more)

### Community 139 - "host-api/src/actions.rs"
Cohesion: 0.08
Nodes (23): Action, Command, CommandArgument, CommandValue, MAX_COMMAND_ARGUMENTS, CommandPermission, key(), MAX_ACTIONS (+15 more)

### Community 140 - "Appearance"
Cohesion: 0.08
Nodes (9): Appearance, MAX_ADDITIONS, MODEL, Catalog, PANTS, SHIRTS, SKINS, decode() (+1 more)

### Community 141 - "Committed"
Cohesion: 0.13
Nodes (8): Committed, Observer, UseObserver, luau_committed_observer_timeout_cannot_block_receipt_or_later_observer_over_listener(), Witness, entity(), present(), seed()

### Community 142 - "presentation.rs"
Cohesion: 0.20
Nodes (15): bounded_float(), Command, command_entity(), display_text(), EntityView, invalid(), optional_bounded_float(), Reply (+7 more)

### Community 143 - "render/mesh.rs"
Cohesion: 0.15
Nodes (11): LightField, ChunkMesh, emit_plant(), emit_quad(), GpuMesh, GpuSubmesh, mesh_chunk_lit(), mesh_chunk_lit_with_catalog() (+3 more)

### Community 144 - "Buffer"
Cohesion: 0.17
Nodes (4): Buffer, capture(), default_filter_preserves_structured_game_events_and_flushes_final_errors(), scoped_filters_and_invalid_filter_fallback_work_without_global_state()

### Community 146 - "public/tests.rs"
Cohesion: 0.12
Nodes (15): BlockExtension, component_schema_package_and_tag_changes_are_compatibility_failures(), Contribute, Declare, Emitter, external_narrow_plant_uses_registered_selection_in_production_raycast(), fixture(), invalid_composition_and_missing_content_fail_atomically_before_installation() (+7 more)

### Community 147 - "drops/entity.rs"
Cohesion: 0.12
Nodes (7): DROP_ENTITY_TYPE, DROP_PAYLOAD_FIXED_BYTES, DropEntityPayload, DropPayloadCodec, MAX_DROP_ENTITY_PAYLOAD_BYTES, register_entity_type(), drop_entity_registration_is_catalog_linked_and_mobile()

### Community 148 - "reaction_removal_tests.rs"
Cohesion: 0.16
Nodes (6): drops(), open_soil(), RemovalDecision, SoilPost, SoilRegistration, stage_reaction()

### Community 149 - "world/generation.rs"
Cohesion: 0.15
Nodes (12): apply(), Builtin, BUILTIN_SAMPLES, builtin_state_key(), BuiltinSamples, compose(), generate_chunk(), generate_chunk_with_contributors() (+4 more)

### Community 150 - "publication/commit.rs"
Cohesion: 0.23
Nodes (9): add_remove(), add_upsert(), collect(), CommitChanges, CommitPlan, fanout_bound(), for_client(), MAX_GROUP_ENTRIES (+1 more)

### Community 151 - ".queue_command"
Cohesion: 0.13
Nodes (8): BINDING_ROWS_PER_PAGE, binding_targets(), BindingTarget, ClientApp, Command, parse(), parse_with_players(), registered()

### Community 152 - "tests/anchored.rs"
Cohesion: 0.14
Nodes (10): owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record(), owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal(), removed(), seed_storage(), storage_startup(), StorageNeighbor, StorageOwner, StorageRemoved (+2 more)

### Community 153 - "HandlerRegistration"
Cohesion: 0.15
Nodes (4): Event, EventKind, HandlerRegistration, RemovalCause

### Community 154 - "InventoryScreen"
Cohesion: 0.09
Nodes (7): InventoryScreen, MAX_SLOTS, MAX_STATUS_FIELDS, SlotGroup, StatusField, StatusFormat, Targets

### Community 155 - "render/drops.rs"
Cohesion: 0.15
Nodes (17): block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting(), DropMeshes, emit_cutout_drop(), flower_pickup_uses_cutout_crosses_instead_of_cube_faces(), grass_side_band_is_at_the_top_on_both_side_axes(), is_sprite_item(), MAX_CUTOUT_INDEX_BYTES, MAX_CUTOUT_VERTEX_BYTES (+9 more)

### Community 156 - "Catalog"
Cohesion: 0.20
Nodes (4): Catalog, error(), texture_definition(), validate_display()

### Community 158 - "custom/shader.rs"
Cohesion: 0.08
Nodes (11): validate(), compose(), STUBS, TYPES, validate(), validate(), compose(), STUBS (+3 more)

### Community 159 - "InventoryProbe"
Cohesion: 0.07
Nodes (12): Blocks, inventories, and processing, InventoryProbe, external_item_action_composed_control_receipt_duplicate_stale_and_restart(), send(), send_with_session(), external_owner_world_read_survives_real_listener_join_and_restart(), external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp(), external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener() (+4 more)

### Community 161 - "Decision"
Cohesion: 0.13
Nodes (6): Behavior, Decision, Event, EventKind, Registration, State

### Community 162 - "reactor/codec.rs"
Cohesion: 0.17
Nodes (9): CodecWorkers, decode_worker(), DECODE_WORKERS, DecodeRequest, encode_worker(), ENCODE_WORKERS, EncodedFrame, EncodeRequest (+1 more)

### Community 163 - "script_startup/creature.rs"
Cohesion: 0.14
Nodes (12): Flat, luau_creature_interaction_and_animation_negotiate_and_keep_private_state(), luau_creature_negotiates_model_ticks_and_restarts(), luau_creature_neighbour_policy_negotiates_and_reads_bounded_public_views(), luau_creature_options_reject_invalid_bounds_before_save_creation(), luau_creature_rejects_invalid_declaration_before_save_and_caught_route_failure(), luau_creature_rejects_invalid_lifecycle_before_movement_or_state_change(), luau_creature_spawns_another_declared_type_with_its_own_initial_state() (+4 more)

### Community 164 - "Growth foundation plan"
Cohesion: 0.10
Nodes (20): 1. Widen identities and introduce block states as one vertical format transition, 2. Deliver the missing parallel gameplay runtime, 3. Complete generic entity lifecycle and prove anchored behavior with gameplay, 4. Scale the real multiplayer path beyond the former 16-client ceiling, 5. Acceptance gate before calling this foundation complete, Baseline before this campaign (historical), Binding technical requirements, Decisions and invariants (+12 more)

### Community 165 - "world/generation/tests.rs"
Cohesion: 0.12
Nodes (11): absolute_anchor_feature_is_not_truncated_at_chunk_boundary(), AcrossSeam, authoritative_generation_edit_baselines_survive_cache_miss_and_restart(), builtin_contributor_preserves_terrain_vegetation_and_negative_chunk_seams(), builtin_contributor_preserves_tree_canopy_across_chunk_seam(), files(), generation_failure_never_installs_air_or_replaces_recovery_snapshot(), generation_identity_mismatch_rejects_before_any_save_mutation() (+3 more)

### Community 166 - "OwnerEffectPatch"
Cohesion: 0.09
Nodes (6): BlockEdit, EditCause, blocked(), EmittedOwnerEffect, OwnerEffectPatch, route_and_consume()

### Community 167 - "entity_checkpoint/tests.rs"
Cohesion: 0.18
Nodes (13): admitted_event_permit_cannot_be_dropped_silently(), checkpoint_io_failure_closes_admission_and_reports_error(), delayed_payload_receipt_keeps_newer_checkpoint_only_motion(), fixture(), interrupted_stream_does_not_publish_and_worker_failure_releases_credit(), malformed_ordered_event_fails_closed_without_checkpoint_publication(), multi_turn_checkpoint_holds_generation_fence_without_a_live_capture(), NEXT_TEST_DIR (+5 more)

### Community 168 - "render/effects.rs"
Cohesion: 0.12
Nodes (11): Descriptor, prepare(), MAX_DESCRIPTOR_BYTES, MAX_PASSES, MAX_SHADER_BYTES, Pass, prepare(), prepare_inner() (+3 more)

### Community 169 - "plan"
Cohesion: 0.24
Nodes (5): capacity_error(), plan(), removal(), removal_cells(), removal_refunds()

### Community 170 - "view.rs"
Cohesion: 0.17
Nodes (14): SlotFilter, draw(), footer(), GOLD, header(), inventory(), machine(), MUTED (+6 more)

### Community 171 - "drops/queries.rs"
Cohesion: 0.22
Nodes (17): age_ms_now(), airborne_count(), capture_nearby(), collect_in_aabb(), distance_sq(), extractable(), has_expired(), live_drop() (+9 more)

### Community 172 - "Result"
Cohesion: 0.23
Nodes (13): catalog_with_extension(), LocalServer, package_catalog_for_preview(), run_server(), run_server_with_limit(), run_server_with_limit_and_catalog(), run_server_with_local_packages(), run_server_with_startup() (+5 more)

### Community 174 - "gameplay/decisions.rs"
Cohesion: 0.09
Nodes (18): acknowledged_result_stays_retired_across_rotation_and_restart(), grant(), inventory_action(), poll_until_settled(), temp_save_dir(), wal_replay_keeps_result_and_world_effect_before_checkpoint(), BLOCK_REGISTER, BLOCK_SOURCE (+10 more)

### Community 175 - "Adapter"
Cohesion: 0.18
Nodes (3): Adapter, offset(), register()

### Community 176 - "handles.rs"
Cohesion: 0.15
Nodes (14): entity(), entity_value(), EntityId, identity_methods(), intern(), profile(), profile_value(), ProfileId (+6 more)

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
Nodes (28): Actions, commands and gameplay decisions, Authored materials, effects and Luau parameters, Authoring tools and runnable examples, Client startup and authored UI, Committed server observations, Content, Durable owner systems, Features requiring engine work or native extensions (+20 more)

### Community 183 - "Composition"
Cohesion: 0.11
Nodes (10): Catalog, Composition, field(), MAX_MEMBERS, MAX_PACKAGES, MAX_TAGS, Package, tag_kind() (+2 more)

### Community 184 - "lighting.rs"
Cohesion: 0.19
Nodes (8): build_bounce(), index(), is_opaque(), MAX_LIGHT, PLANE, propagate(), SIDE, VOLUME

### Community 185 - "ClientApp"
Cohesion: 0.09
Nodes (6): chunk_in_view(), ClientApp, ClientApp, lighting_depends_on(), mesh_priority(), LightSample

### Community 190 - "EntityDefinition"
Cohesion: 0.08
Nodes (4): EntityDefinition, EntityState, Catalog, Catalog

### Community 191 - "snapshots.rs"
Cohesion: 0.19
Nodes (9): apply(), dispatch(), finish(), MAX_SNAPSHOT_JOBS, Prepared, publish(), Selection, Target (+1 more)

### Community 192 - "script_startup/gameplay.rs"
Cohesion: 0.13
Nodes (8): Fixture, luau_action_loopback_rollbacks_exact_transfer_receipts_and_restart(), luau_action_planner_errors_and_unavailable_retry_are_atomic(), luau_action_registration_and_persisted_source_identity_fail_closed(), Peer, PROFILE, REGISTER, SOURCE

### Community 193 - "client/bundle.rs"
Cohesion: 0.17
Nodes (7): CACHE, DeadlineStream, install(), invalid(), receive(), receive_progress(), TEST_CACHE_LOCK

### Community 194 - "sealed_neighborhood"
Cohesion: 0.13
Nodes (10): key_offset(), incoming(), bounced_mode_reflects_surface_color_without_leaking_into_default(), distant_streamed_roof_blocks_and_reopens_a_deep_shaft(), emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides(), mapped_glowstone_definition_supplies_emission_to_light_builder(), opening_a_roof_shaft_relights_the_cave(), plants_and_leaves_transmit_daylight() (+2 more)

### Community 195 - "Clock"
Cohesion: 0.08
Nodes (7): Clock, Capture, Clock, DOMAIN, publish(), ReadStamp, state_key()

### Community 196 - "parallel/tests.rs"
Cohesion: 0.21
Nodes (16): batch(), bounded_queue_reports_saturation_without_accepting_a_partial_job(), cancellation_skips_queued_work_and_marks_running_results_cancelled(), completed_jobs(), dependency_waves_can_commit_twice_in_one_tick_and_later_wave_sees_prior_result(), job_errors_and_panics_reach_the_barrier_and_the_pool_keeps_running(), key(), owner_at() (+8 more)

### Community 197 - "Proposal: one coherent gameplay API"
Cohesion: 0.11
Nodes (19): 10. Custom models: required direction, deferred work, 1. A small, powerful set of concepts, 2. Atomic operations are a convenience, not a burden, 3. Specialized APIs are optional conveniences, 4. Built-in gameplay uses the same boundary, 5. Distinguish genuinely different execution contexts, 6. Developer experience is part of the implementation, 7. Server-delivered mod packages (+11 more)

### Community 198 - "Sender"
Cohesion: 0.12
Nodes (8): channel(), Sender, Shared, State, background_backlog_cannot_fill_edit_capacity_and_invalidation_removes_queued_work(), hot_edit_coalesces_and_promotes_without_losing_background_progress(), job(), shutdown_wakes_idle_workers()

### Community 199 - "material.rs"
Cohesion: 0.19
Nodes (15): blend_opposite_pixels(), emission_strengths(), face_uv(), item_material_layer(), item_material_layer_for(), material_layer(), material_layer_for(), material_mips() (+7 more)

### Community 200 - "Adapter"
Cohesion: 0.16
Nodes (4): Adapter, error(), register(), spawn_clear()

### Community 201 - "mlua"
Cohesion: 0.12
Nodes (6): command_declaration(), command_schema(), Declaration, declarer(), registration(), ScriptHandler

### Community 202 - "Runtime"
Cohesion: 0.17
Nodes (4): count(), number(), own_key(), Runtime

### Community 203 - "declarer"
Cohesion: 0.27
Nodes (4): declarer(), field(), number(), triple()

### Community 204 - "EntityTransferPolicy"
Cohesion: 0.06
Nodes (11): Adapter, Adapter, Adapter, AutomationStack, EntityItemTransfer, EntityTransferPolicy, movable_count(), move_up_to() (+3 more)

### Community 205 - "players/lifecycle.rs"
Cohesion: 0.14
Nodes (11): committed(), drive(), enqueue(), Job, joined(), key(), leaving(), Published (+3 more)

### Community 206 - "Update"
Cohesion: 0.19
Nodes (9): decode(), defaults(), Definition, identifier(), Kind, MAX_PARAMETERS, State, Update (+1 more)

### Community 207 - "client/startup.rs"
Cohesion: 0.25
Nodes (9): ascii(), display(), execute(), execute_event(), identity(), load(), prepare(), run() (+1 more)

### Community 208 - "VisualFire"
Cohesion: 0.13
Nodes (8): FireRenderer, FLOATS, MAX_BYTES, MAX_FIRES, triangle(), vertices(), VERTICES_PER_FIRE, VisualFire

### Community 209 - "render/tests.rs"
Cohesion: 0.11
Nodes (8): mesh_chunk(), adjacent_leaves_skip_interior_cutout_faces(), grass_side_is_upright_on_both_wall_axes(), greedy_quads_repeat_material_once_per_voxel(), mapped_builtin(), meshing_uses_shared_chunk_layout_and_world_origin(), plants_have_two_crossed_cutout_quads_and_do_not_hide_ground(), remapped_connection_catalog_drives_foliage_meshes_and_drop_art()

### Community 210 - "machine_component_tests.rs"
Cohesion: 0.19
Nodes (9): exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers(), independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart(), load_neighbours(), payload(), public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports(), pulse(), SelectiveMachine, settle() (+1 more)

### Community 211 - "register"
Cohesion: 0.13
Nodes (4): register(), Slots, StoragePayload, StoragePayload<N>

### Community 212 - "World"
Cohesion: 0.07
Nodes (19): Mossbun, BODY, gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling(), ground_motion_respects_walls_cliffs_seams_and_embedded_edits(), view(), World, World<'a>, BODY (+11 more)

### Community 213 - "process_movement_batch"
Cohesion: 0.32
Nodes (17): MovementCommand, process_movement_batch(), air_chunk(), close(), command(), command_order_is_stable_and_affects_the_authoritative_position(), invalid_and_excessive_deltas_consume_sequence_without_changing_position(), key() (+9 more)

### Community 214 - "SystemDescriptor"
Cohesion: 0.05
Nodes (30): access(), AccessKind, BudgetKind, depends_on(), ExecutableSystem, IdentifierError, MAX_DEPENDENCIES_PER_SYSTEM, MAX_NEIGHBOR_RADIUS (+22 more)

### Community 215 - "neighborhood.rs"
Cohesion: 0.16
Nodes (16): luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect(), luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes(), luau_neighborhood_radius_requires_exact_bounds_and_world_capability(), fixture(), GROW, id(), inbox(), load_neighborhoods() (+8 more)

### Community 216 - "streaming.rs"
Cohesion: 0.15
Nodes (12): OutboundClientSnapshot, can_stream_snapshot(), can_stream_snapshot_size(), MAX_LOAD_RESULTS_PER_TICK, MAX_NEW_LOADS_PER_CLIENT, MAX_PREFETCH_CANDIDATES, poll_chunk_loads(), publish_streams() (+4 more)

### Community 217 - "CoordinatorContext"
Cohesion: 0.21
Nodes (14): durable_actions(), fire_delivery(), fire_source(), input_authorization(), interaction_commit(), player_movement(), publish(), apply_simulation_input() (+6 more)

### Community 218 - "GpuPass"
Cohesion: 0.12
Nodes (3): Data, GpuPass, texture_entry()

### Community 219 - "StorageBlockEntity"
Cohesion: 0.13
Nodes (10): FootprintCell, MAX_FOOTPRINT, MAX_STORAGE_SLOTS, PlacementContext, PlaceStorage, RemovalContext, RemoveStorage, StorageBlockEntity (+2 more)

### Community 220 - ".compose_current_package_action_with_args"
Cohesion: 0.19
Nodes (9): ActionChoice, ClientApp, compose_named_command(), compose_observed_entity_action(), compose_package_action(), compose_package_action_with_args(), PackageActionInput, authored_entity_action_uses_observed_identity_and_exact_bounded_arguments() (+1 more)

### Community 221 - "wake.rs"
Cohesion: 0.14
Nodes (11): blocked(), canonical_wakes(), EntityWake, interact_producer(), INTERACT_PRODUCER_ID, MAX_WAKES_PER_PLAN, register_wake_kind(), route_wakes() (+3 more)

### Community 222 - "VisualAvatar"
Cohesion: 0.08
Nodes (10): ActorAnimator, DELAY, Sample, STEP, Track, AvatarInstance, AvatarModel, AvatarRenderer (+2 more)

### Community 223 - "Bindings"
Cohesion: 0.23
Nodes (7): Action, allowed_key(), Bindings, letter(), NamedBindings, parse(), valid_action_key()

### Community 224 - "ContentManifest"
Cohesion: 0.19
Nodes (9): checksum(), ContentEntry, ContentManifest, invalid(), MAGIC, MAX_ENTRIES, MAX_MANIFEST_BYTES, valid_state_key() (+1 more)

### Community 225 - "Preparation"
Cohesion: 0.17
Nodes (3): Preparation, Ready, Renderer

### Community 226 - "prepare"
Cohesion: 0.18
Nodes (6): decode_motion_id(), invalid_data(), invalid_entity(), prepare(), PreparedEntityRecovery, validate_checkpointed_motion()

### Community 227 - "Stack"
Cohesion: 0.15
Nodes (6): ComponentPayload, HOTBAR_SLOTS, MAX_COMPONENT_BYTES, SLOTS, Stack, STACK_LIMIT

### Community 229 - "position_store.rs"
Cohesion: 0.19
Nodes (9): checksum(), invalid(), LEN, MAGIC, PositionStore, TEMP_SEQUENCE, validate_position(), validate_profile() (+1 more)

### Community 230 - "services.rs"
Cohesion: 0.19
Nodes (9): coordinate(), guarded(), integer_cell(), invoke(), LifecycleRequest, parse_lifecycle(), parse_spawn(), SpawnRequest (+1 more)

### Community 231 - "modding/README.md"
Cohesion: 0.18
Nodes (7): Growth foundation: remaining implementation, Player rules, Public persistent owner systems, Phase 6 UI foundation, Documentation, Durable neighborhood relay, Targeted authored UI fixture

### Community 232 - "Context<'_>"
Cohesion: 0.31
Nodes (4): Context<'_>, Entity, EntityChange, EntitySpawn

### Community 233 - "durable/checkpoint.rs"
Cohesion: 0.24
Nodes (9): checkpoint_keys_turn(), decode_chunk_checkpoint_key(), decode_inventory_checkpoint_key(), fire_batch_key(), is_fire_checkpoint_key(), process_checkpoint_receipts(), submit_dirty_checkpoints(), submit_fire_checkpoint_batch() (+1 more)

### Community 234 - "inventory"
Cohesion: 0.15
Nodes (6): expiry_due_prefix_is_capped_and_uncommitted_work_remains_eligible(), fill_chunk(), motion_is_scheduled_entity_work_never_coordinator_stepping(), one_moving_drop_costs_its_own_records_never_the_population(), staged_motion_bytes(), test_store()

### Community 235 - "script_startup/appearance.rs"
Cohesion: 0.17
Nodes (6): appearance_bundle_composes_with_rules_animation_and_exact_world_identity(), CALL, invalid_or_caught_appearance_declarations_never_publish(), Peer, registered_appearance_selection_replicates_and_restarts_by_profile(), source()

### Community 236 - "pipeline.rs"
Cohesion: 0.21
Nodes (6): create_custom_voxel_pipeline(), create_voxel_pipeline(), create_voxel_pipeline_source(), create_voxel_pipeline_with_catalog(), SHADER, VERTEX_STRIDE

### Community 237 - "gameplay_anchor_tests.rs"
Cohesion: 0.21
Nodes (6): action_and_tick_expand_secondary_cell_once_and_refund_final_inventory_on_restart(), AnchorExtension, barrier(), commit_command(), open_anchor(), withdraw()

### Community 238 - "entity_sleep.rs"
Cohesion: 0.30
Nodes (19): process_durable_actions(), queue_interaction_actions(), dispatch(), edit(), empty_action(), live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery(), new_sleepers_cannot_extend_the_current_recheck_pass(), position() (+11 more)

### Community 239 - "EntityDependencies"
Cohesion: 0.16
Nodes (3): EntityDependencies, EntityStore, PreparedEntityTransaction

### Community 240 - "Execution foundation: next implementation slices"
Cohesion: 0.12
Nodes (16): 1. Scheduling and progress under capacity pressure, 2. Reliable wake and sleep semantics, 3. Separate conflict revisions from publication ordering, 4. Consolidate commit orchestration and make barriers explicit, 5. Off-thread publication and bounded checkpoint work, Execution approach, Execution foundation: next implementation slices, First: review the current worker slice — done (+8 more)

### Community 241 - "EffectKindId"
Cohesion: 0.19
Nodes (4): EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen, EffectRegistryError

### Community 242 - "script_startup.rs"
Cohesion: 0.19
Nodes (7): CONTENT, Fixture, luau_failed_restart_leaves_existing_save_unchanged_and_can_retry(), luau_startup_item_reaches_listener_inventory_and_restart(), luau_startup_rejections_publish_nothing_and_never_open_world(), luau_startup_validates_contracts_and_runs_imports_at_the_item_bound(), TOKEN

### Community 243 - "script_startup/bundle.rs"
Cohesion: 0.24
Nodes (15): bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join(), bundle_restart_exact_cache_and_changed_source_require_new_bytes(), client_verification_rejects_relay_tamper_truncation_and_reordering(), closed(), fixture(), fragmented(), offer(), package_effect_is_prepared_by_real_client_join_before_welcome() (+7 more)

### Community 244 - "FirePending"
Cohesion: 0.12
Nodes (12): checked_body(), checksum(), finish(), key_bytes(), read_key(), decode_pending_key(), FireIgnition, FireIgnitionId (+4 more)

### Community 245 - "integer"
Cohesion: 0.11
Nodes (12): coordinate(), Declaration, declarer(), invoke(), registration(), runtime(), ScriptContributor, parse() (+4 more)

### Community 247 - "authored.rs"
Cohesion: 0.08
Nodes (15): ATLAS_SIZE, Document, identifier(), INVALID, json(), MAX_TEXT, owned(), Resources (+7 more)

### Community 248 - "src/composition.rs"
Cohesion: 0.11
Nodes (14): ACTIONS, ANCHORED_ENTITIES, Bundle, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, ITEM_ICONS (+6 more)

### Community 250 - "io"
Cohesion: 0.11
Nodes (6): TURN_ENTRIES, WRITE_BYTES, plan_spawn(), validate_spawn(), MAX_FRONTIER_CELLS, AGE_BUCKETS

### Community 251 - "client/lifecycle/tests.rs"
Cohesion: 0.26
Nodes (9): exercise_join_lifecycle(), exercise_player_services(), join(), player_notices_present_only_current_session_and_kicks_retire_it(), player_roster_accepts_newer_snapshots_and_is_cleared_when_session_retires(), public_player_snapshots_validate_session_and_services_then_clear_on_retirement(), read(), retired() (+1 more)

### Community 252 - "bundle_ui.rs"
Cohesion: 0.19
Nodes (8): event(), client_startup_failure_refuses_content_ready_with_package_and_module(), downloaded_client_startup_is_session_scoped_across_reconnect_and_switch(), downloaded_replica_visuals_use_exact_entity_ids_and_reset_on_switch(), startup_fixture(), startup_worker_discards_partial_registration_and_caught_limit(), startup_worker_imports_exact_direct_dependencies_with_lexical_visibility(), verified_replica_callbacks_are_session_scoped_worker_presentations()

### Community 253 - "Cross-cutting integration findings"
Cohesion: 0.08
Nodes (19): Approved unified gameplay implementation, Audit boundary and live path, Baseline and verdict, Built-in capability parity audit, Capability matrix, Cross-cutting integration findings, Definitions, assets, and composition, Evidence and integration acceptance (+11 more)

### Community 254 - "package/tests.rs"
Cohesion: 0.25
Nodes (11): cycles_depth_and_shared_execution_budget_are_bounded(), dependency_versions_and_manifest_declarations_are_strict(), discovery_bounds_directory_count_source_bytes_and_total_bytes(), discovery_rejects_symlinks_at_every_path_level_and_special_files(), failed_modules_are_not_reinitialized_when_caught(), Fixture, import_failures_name_package_version_and_module_and_do_not_poison_worker(), imported_source_limits_and_exported_function_errors_keep_source_identity() (+3 more)

### Community 255 - "install"
Cohesion: 0.27
Nodes (8): amount(), decode_stack(), field(), index(), install(), latch(), owner(), stack_table()

### Community 257 - "slot.rs"
Cohesion: 0.26
Nodes (4): color_from_swatch(), paint_icon(), show(), SlotStyle

### Community 258 - "invoke"
Cohesion: 0.19
Nodes (7): Capabilities, captured_entity(), checked(), entity_identity(), invoke(), push_wake(), read_block()

### Community 259 - ".draw_node"
Cohesion: 0.22
Nodes (3): Intent, rgba(), Session

### Community 260 - "DroppedItem"
Cohesion: 0.31
Nodes (5): DropAnimator, live_visual(), PickupFlight, POSITION_BLEND, DroppedItem

### Community 262 - "Gpu"
Cohesion: 0.10
Nodes (4): Data, Gpu, MaterialData, updates_are_owned_typed_atomic_and_coalesced()

### Community 263 - "PlayerSummary"
Cohesion: 0.21
Nodes (7): complete(), normalize(), token(), PlayerSummary, read(), validate_notice(), write()

### Community 265 - "Hit"
Cohesion: 0.09
Nodes (27): no_hit(), no_interact(), player_adapter(), PLAYER_ENTITY_TYPE, project_avatar(), INSERT_FUEL, INSERT_INPUT, interact_verb() (+19 more)

### Community 266 - "system/intents.rs"
Cohesion: 0.18
Nodes (12): luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id(), luau_intent_send_requires_opt_in_and_absence_requires_bootstrap(), CHAIN, id(), inbox(), luau_intent_absent_destinations_run_on_real_listener_and_recover_once(), luau_intent_caught_invalid_and_overbudget_sends_poison_all_output(), luau_intent_declarations_and_session_identity_are_owned_and_bounded() (+4 more)

### Community 267 - "Input"
Cohesion: 0.22
Nodes (7): inside_view(), apply(), disconnect(), Input, prepare(), Prepared, reduce_view_under_pressure()

### Community 268 - "client/world.rs"
Cohesion: 0.22
Nodes (7): Cursor, MAX_PENDING_GROUPS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot, WorldProbe, WorldUpdate

### Community 269 - "registry/tests.rs"
Cohesion: 0.18
Nodes (19): builtin_phase_plan(), register_builtin_systems(), declarations_match_the_current_execution_shape(), descriptor(), deterministic_plan(), disjoint_and_read_only_accesses_can_share_a_phase(), duplicate_ids_and_invalid_namespaced_ids_are_rejected(), freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() (+11 more)

### Community 271 - "Imports"
Cohesion: 0.17
Nodes (3): Imports, MAX_IMPORT_DEPTH, ModuleState

### Community 272 - "sync"
Cohesion: 0.20
Nodes (3): OwnerApplyReceipt, OwnerApplyTask, World

### Community 273 - "decode_transaction"
Cohesion: 0.20
Nodes (11): crc32(), decode_transaction(), encode_frame(), frame_checksum(), frame_len(), invalid_data_owned(), invalid_input(), Reader (+3 more)

### Community 274 - "world_time/tests.rs"
Cohesion: 0.39
Nodes (5): clock_command_recovers_past_an_older_checkpoint_and_rotated_wal(), local_listener_delivers_shared_world_time_admin_changes_and_recovers_it(), save(), temporary(), world_time_resumes_saved_phase_and_rejects_corrupt_state()

### Community 275 - "package/manifest.rs"
Cohesion: 0.18
Nodes (8): asset_path(), bounded_path(), identifier(), Manifest, public_path(), SourceSide, valid_path(), valid_version()

### Community 276 - "Item"
Cohesion: 0.11
Nodes (4): Components, DropSize, Item, Catalog

### Community 277 - "server/appearance.rs"
Cohesion: 0.23
Nodes (5): checksum(), LEN, select(), SEQUENCE, Store

### Community 278 - "arc"
Cohesion: 0.15
Nodes (12): anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic(), command(), edit(), fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record(), KEY, open(), public(), resident() (+4 more)

### Community 280 - "Scripting Gap Closure Plan"
Cohesion: 0.15
Nodes (13): Coverage and implementation order, Design requirements, Milestone 1 Authoring foundations, Milestone 2 Gameplay control and native parity, Milestone 3 World jobs and structured observations, Milestone 4 Dynamic UI, Milestone 5 Audio, Milestone 6 Models geometry and physics (+5 more)

### Community 282 - ".public_view"
Cohesion: 0.35
Nodes (4): AppearanceCodec, encode_stack(), StackPayload, StackPayloadCodec

### Community 286 - "Result"
Cohesion: 0.35
Nodes (6): invalid_data(), read_world_metadata(), resolve_content_map_with(), verify_content_map_with(), verify_new_world_directory(), write_content_map()

### Community 287 - "owner_durable/tests.rs"
Cohesion: 0.36
Nodes (15): byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation(), capacity_defers_while_corruption_stops(), chunk(), commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing(), committed_waves_mark_active_and_update_the_due_index(), counter_store(), dropped_prepared_wave_changes_nothing(), fed_deadlines_are_not_duplicated_and_replay_clears_stale_readiness() (+7 more)

### Community 288 - "server/checkpoint/tests.rs"
Cohesion: 0.27
Nodes (14): capacity_counts_running_jobs_and_unconsumed_receipts(), closure_panic_becomes_an_error_receipt_and_worker_keeps_running(), completed_but_unconsumed_receipt_still_occupies_capacity(), drop_drains_accepted_work_without_blocking_on_full_receipt_channel(), failed_write_is_returned_with_its_key_and_revision(), independent_checkpoint_keys_progress_while_another_shard_is_blocked(), key(), key_on_shard() (+6 more)

### Community 290 - "route_registered_effects"
Cohesion: 0.27
Nodes (12): RegisteredEffectLimits, route_registered_effects(), chunk(), emitted(), invalid_payload_and_duplicate_destinations_are_rejected(), keys(), oversized_expanded_fanout_aborts_routing_before_any_batch_is_returned(), registered_consumer_builds_a_typed_scratch_patch_for_its_destination() (+4 more)

### Community 291 - "showcase.rs"
Cohesion: 0.16
Nodes (10): phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart(), PRESS_ANCHOR, send_action(), asset_aggregate_bytes_and_declaration_count_are_bounded(), asset_set_count_is_bounded_even_for_empty_files(), assets_use_secure_bounded_regular_file_reads(), classified(), classified_packages_preserve_clientless_startup_and_server_import_authority() (+2 more)

### Community 292 - "join_named_client"
Cohesion: 0.16
Nodes (5): join_client(), JoinReply, JoinResponse, PendingJoin, join_named_client()

### Community 293 - "ItemIcon"
Cohesion: 0.16
Nodes (3): ItemIcon, definitions(), Catalog

### Community 294 - "stage_wave"
Cohesion: 0.36
Nodes (8): apply_synced_batch(), Durability, run_delivery(), run_source(), stage_gameplay_burn(), stage_transactions(), stage_wave(), is_checkpoint_key()

### Community 296 - "Harvest"
Cohesion: 0.33
Nodes (3): Harvest, Pickup, PlantSupport

### Community 298 - "script_startup/machine.rs"
Cohesion: 0.13
Nodes (15): LifecyclePlan, Machine, luau_machine_footprint_negotiates_across_seam_and_restarts(), luau_machine_footprint_places_and_breaks_secondary_cell_over_listener(), luau_machine_negotiates_plans_and_restarts(), luau_machine_ports_and_transfer_work_negotiate_and_restart(), luau_machine_recipe_list_negotiates_filters_and_restarts(), luau_machine_recipe_list_rejects_overlapping_inputs_before_save() (+7 more)

### Community 300 - "request_chunk"
Cohesion: 0.24
Nodes (7): denied(), Invocation, plan(), read_target(), sight(), verify_reach(), request_chunk()

### Community 301 - "Inputs"
Cohesion: 0.16
Nodes (3): expand(), Expansion, Inputs

### Community 302 - "durable/fire/tests.rs"
Cohesion: 0.35
Nodes (6): check_seeded_fire_restart(), checkpoint_pressure_admits_a_durable_owner_prefix_without_losing_the_remainder(), dirty_fire_keys_use_one_checkpoint_job_and_revision_fenced_receipt(), drain_wal(), fire_burn_uses_public_removal_and_support_handlers_in_one_receipt(), temp_save()

### Community 303 - "admin/tests.rs"
Cohesion: 0.13
Nodes (4): generic_parser_uses_ordered_negotiated_schema_not_builtin_names(), player_command_names_completion_and_reconnect_use_exact_sessions(), request(), command_signature()

### Community 305 - "Downloads"
Cohesion: 0.10
Nodes (4): complete_package_handshake(), Downloads, receive_package(), state_with_package()

### Community 306 - "receive_content_manifest"
Cohesion: 0.07
Nodes (20): clock(), external_system_runs_without_entities_and_recovers_across_real_listener_restart(), startup(), receive_content_manifest(), authored_button_reaches_authoritative_receipt_and_durable_inventory(), commit(), custom_drop_policy_negotiates_live_expiry_and_preserves_birth_across_restart(), luau_machine_active_state_negotiates_and_preserves_save_identity() (+12 more)

### Community 308 - "slots"
Cohesion: 0.39
Nodes (4): apply(), capture(), slots(), stack()

### Community 309 - "inventory/container.rs"
Cohesion: 0.24
Nodes (5): decode(), encode(), invalid(), max_bytes(), independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data()

### Community 311 - "world"
Cohesion: 0.18
Nodes (7): AIM, combined_mod_downloads_acts_grows_and_recovers(), combined_mod_two_profiles_act_independently_and_recover(), GROW, grow_once(), open(), PROFILE

### Community 312 - "declarer"
Cohesion: 0.14
Nodes (3): identifier(), declarer(), Event

### Community 313 - "server/drops.rs"
Cohesion: 0.14
Nodes (8): DROP_RADIUS, GRAVITY, invalid(), is_drop_delta(), LIFETIME, TERMINAL_SPEED, VIEW_RANGE, VIEW_RANGE_SQ

### Community 314 - "client_metadata.rs"
Cohesion: 0.24
Nodes (4): Catalog, Entity, Identity, Metadata

### Community 315 - "BundleIdentity"
Cohesion: 0.30
Nodes (8): BundleIdentity, CLIENT_RUNTIME_VERSION, MAX_BUNDLE_PART, read_identity(), read_part(), validate_part(), write_identity(), write_part()

### Community 316 - "raycast_blocks"
Cohesion: 0.22
Nodes (7): aiming_past_grass_edges_reaches_ground_but_center_hits_flower(), integer_plane_moving_negative_starts_in_the_entered_voxel(), parallel_axis_uses_half_open_boundary_ownership(), raycast_blocks(), reports_target_face_and_adjacent_cell(), simultaneous_corner_crossing_advances_all_axes(), traverses_negative_coordinates_and_negative_faces()

### Community 319 - "entities/container.rs"
Cohesion: 0.27
Nodes (3): Codec, ContainerPayload, register()

### Community 320 - "script_startup/gameplay/entities.rs"
Cohesion: 0.19
Nodes (7): Fixture, luau_entity_schema_loopback_spawn_due_callback_and_recovery(), luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling(), prepare(), REGISTER, REGISTER_ENTITY, SOURCE

### Community 321 - "script_startup/gameplay/inventory.rs"
Cohesion: 0.21
Nodes (10): actor(), luau_automatic_pickup_exact_components_conservation_and_restart(), luau_component_schema_rejects_wrong_payload_and_persists_exact_bytes(), luau_inventory_exact_binary_reads_give_take_and_error_rollback_restart(), luau_pickup_host_credit_eligibility_permissions_and_caught_errors_rollback(), luau_take_and_spawn_stack_preserve_binary_components_across_receipt_and_restart(), PICKUP, PICKUP_REGISTER (+2 more)

### Community 322 - "items.rs"
Cohesion: 0.30
Nodes (8): only_block_items_are_placeable(), placeable_block(), placeable_block_in(), SAPLING, SEEDS, STICK, valid_item(), valid_item_in()

### Community 323 - "position_store/tests.rs"
Cohesion: 0.21
Nodes (5): corrupted_position_is_not_silently_replaced(), position_checkpoint_does_not_share_inventory_temp_namespace(), position_round_trips_and_is_profile_scoped(), SEQUENCE, TestSave

### Community 325 - "complete_barrier"
Cohesion: 0.18
Nodes (12): advance_receipts(), CommitBarrier, CommitProgress, complete_barrier(), drain_staged_receipts(), flush_ready_fire(), poll_journal_receipts(), caught_invalid_burn_removal_decision_rejects_whole_owner_wave() (+4 more)

### Community 326 - "FireAnimator"
Cohesion: 0.23
Nodes (4): FireAnimator, LIFE, MAX_DISTANCE_SQUARED, MAX_FIRES

### Community 328 - "queries/tests.rs"
Cohesion: 0.33
Nodes (7): airborne_count_tracks_schedule_not_records(), expired_drop_is_visible_but_never_pickable(), immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation(), pickup_delay_gates_candidates_but_not_visibility(), single_drop_collection_rechecks_range_delay_and_expiry(), spawn_direct(), test_store()

### Community 329 - "Player"
Cohesion: 0.16
Nodes (6): Context<'_>, Player, PlayerOperation, PlayerOperationKind, capture(), publish_roster()

### Community 331 - "2. Player and lifecycle hooks"
Cohesion: 0.10
Nodes (20): 1. Basic runtime tools — closed, 2. Player and lifecycle hooks, 3. Dynamic UI and input, 4. General persistent block entities, 5. Flexible entities, motion and presentation, 6. Development iteration and save continuity, 7. Content-pack scale and composition limits, Additional scope accepted for implementation (+12 more)

### Community 334 - "script_startup/generation.rs"
Cohesion: 0.24
Nodes (8): Fixture, GENERATION, luau_generation_baseline_edits_and_identity_survive_restart(), luau_generation_rejects_bad_registration_and_caught_output_errors(), luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads(), luau_generation_streams_from_loader_after_restart(), MARKER, REGISTER

### Community 335 - "tcp/report.rs"
Cohesion: 0.31
Nodes (5): TransportSnapshot, OutboundSnapshot, percentile(), summarize(), TcpSoakReport

### Community 336 - "Value"
Cohesion: 0.11
Nodes (15): Value, bytes(), decode(), delay(), boolean(), cube(), extended(), has_state() (+7 more)

### Community 337 - "streaming/entities.rs"
Cohesion: 0.19
Nodes (5): MAX_PUBLIC_ENTITIES_PER_CHUNK, MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK, project(), snapshot_messages(), SnapshotError

### Community 338 - "publication.rs"
Cohesion: 0.33
Nodes (7): apply_committed_action(), apply_committed_action_inner(), apply_committed_fire_action(), apply_committed_owner_world(), is_owner_publication_key(), publish_committed(), publish_committed_fire_after_world()

### Community 339 - "src/actions/tests.rs"
Cohesion: 0.22
Nodes (6): action(), command_facets_are_empty_gameplay_only_and_fingerprint_permissions(), composed_controls_resolve_forward_references_without_changing_target_context(), composition_bounds_and_fingerprint_cover_every_control(), discovery_is_bounded_ordered_and_rejects_conflicting_ownership(), ordered_command_schema_has_canonical_bounded_arguments_and_identity()

### Community 341 - "lifecycle-fixture/src/machine.rs"
Cohesion: 0.20
Nodes (5): Crush, KEY, MARKED_INPUT, REFINED_INPUT, register()

### Community 342 - "journal/recovery.rs"
Cohesion: 0.30
Nodes (7): Journal, legacy_header(), open_or_create_legacy(), sync_parent(), truncate_tail(), validate_legacy_header(), write_legacy_header()

### Community 343 - "player_services/tests.rs"
Cohesion: 0.22
Nodes (4): client_player_callback_rejection_is_atomic_and_other_packages_keep_running(), client_player_callbacks_compose_filter_public_state_and_disconnect_without_queue_room(), Fixture, states()

### Community 344 - "Bloxgloom interface plan"
Cohesion: 0.25
Nodes (8): Baseline when this plan was written, Bloxgloom interface plan, Delivery order, Goal, Interaction contract, Performance and correctness, UI and game-state design, Validation record

### Community 345 - "DropPolicy"
Cohesion: 0.22
Nodes (3): DropPolicy, .BYTE_LEN, .MAX_PICKUP_RANGE

### Community 346 - "materials.rs"
Cohesion: 0.29
Nodes (7): bound_texture_asset_must_decode_and_match_owned_canonical_metadata(), bundle(), DESCRIPTOR, material_bundle_verifies_key_ownership_limits_and_catalog_readiness(), SHADER, texture_metadata(), version_two_material_verifies_hooks_parameters_and_negotiated_layers()

### Community 347 - "navigation.rs"
Cohesion: 0.13
Nodes (8): MAX_NODES, RADIUS, Route, nearest_unsent(), boundaries_skip_unrepresentable_chunk_keys(), distance(), sent_keys_are_skipped_without_expanding_the_budget(), visits_every_interest_key_once_in_distance_order()

### Community 349 - "commands.rs"
Cohesion: 0.24
Nodes (7): advertised_builtin_commands_and_compatibility_packets_share_auth_receipts_and_restart(), command_request(), declaration(), invalid_command_declarations_poison_startup_even_when_caught(), negotiated_commands_enforce_permission_and_zero_args_with_receipts_and_restart(), typed_mod_command_negotiates_order_validates_before_handler_and_recovers_once(), typed_request()

### Community 350 - "script_startup/player.rs"
Cohesion: 0.26
Nodes (8): check_movement(), custom_player_rules_negotiate_before_welcome_and_survive_restart(), FIELDS, player_artifact_tampering_is_rejected_or_fails_exact_manifest_match(), player_rules_and_drop_animation_share_one_verified_bundle_before_join(), player_rules_compose_with_existing_sized_item_artifacts(), rejected_player_declarations_including_caught_errors_never_open_world(), source()

### Community 351 - "owner_wave/tests.rs"
Cohesion: 0.44
Nodes (9): a_system_wave_cannot_commit_two_patches_for_the_same_owner(), batch(), chunk(), handler_or_budget_failure_aborts_the_entire_wave(), patch(), results(), system(), validated_wave_applies_in_canonical_owner_order_after_aggregate_checks() (+1 more)

### Community 352 - "create"
Cohesion: 0.25
Nodes (6): create(), author_reseeding_keeps_standard_math_random_semantics(), diagnostic_encoding_rejects_nested_fields_without_executing_metamethods(), failed_attempts_keep_diagnostics_and_helper_source_identity(), logging_pressure_does_not_change_random_results_and_retries_repeat_attempts(), ordinary_libraries_and_seeded_random_work_in_fresh_sandboxes()

### Community 354 - "register"
Cohesion: 0.33
Nodes (3): definition(), KEY, register()

### Community 355 - "Bloxgloom"
Cohesion: 0.22
Nodes (9): Bloxgloom, Client execution and remaining extension work, Current execution architecture, Development and previews, HDR presentation, Publication and checkpoint boundaries, Run locally, Server threads and workers (+1 more)

### Community 356 - "server/perf.rs"
Cohesion: 0.36
Nodes (6): run_fire_cpu_perf(), run_fire_perf(), run_perf_benchmark(), run_scenario(), run_tcp_perf(), validate_tick_count()

### Community 357 - "drop_merge.rs"
Cohesion: 0.24
Nodes (4): DropMergeCandidate, DropMergeContext, DropStackFill, filling_and_splitting_conserve_items_at_the_stack_cap()

### Community 359 - "inventory/store.rs"
Cohesion: 0.22
Nodes (7): HEADER_LEN, MAGIC, MAX_LEN, MIN_LEN, SLOT_FIXED_LEN, TEMP_SEQUENCE, VERSION

### Community 360 - "refund_stacks"
Cohesion: 0.21
Nodes (4): place(), refund_removal(), refund_stacks(), remove()

### Community 361 - "script_startup/machine/components.rs"
Cohesion: 0.39
Nodes (5): luau_component_machine_processes_exact_stack_over_listener_and_recovers(), luau_machine_component_options_reject_invalid_constants_before_save(), luau_machine_component_recipe_negotiates_exact_predicate_and_preservation(), luau_machine_present_input_exact_output_and_component_fuel_roundtrip(), source()

### Community 362 - "Registered inventory views and screens"
Cohesion: 0.33
Nodes (6): Generic client and server paths, Independent persistence and bounds, Public registration, Registered inventory views and screens, Try the external fixture, Verification

### Community 363 - "perf/report.rs"
Cohesion: 0.46
Nodes (4): PHASE_NAMES, print_latency(), print_metric(), print_report()

### Community 365 - "decode"
Cohesion: 0.27
Nodes (3): decode(), encode(), Format

### Community 366 - "src/storage/tests.rs"
Cohesion: 0.29
Nodes (9): active_world_lock_excludes_a_second_writer_and_releases_on_drop(), content_map_preserves_wide_assignments_and_rejects_reassignment(), hidden_partial_conversion_stage_cannot_be_opened_even_after_marker_removal(), incomplete_conversion_cannot_be_opened_as_a_world(), old_world_is_rejected_without_creating_a_lock_or_rewriting_data(), temporary_root(), wide_sparse_edits_round_trip_and_reject_reordering(), with_extra_block() (+1 more)

### Community 368 - ".first_solid_top"
Cohesion: 0.42
Nodes (3): FallingContext, FallingPlan, FallingWorld

### Community 369 - "lifecycle-fixture/src/content.rs"
Cohesion: 0.20
Nodes (6): CHIP, Content, LAMP, PNG, REED, TEXTURE

### Community 370 - "std"
Cohesion: 0.08
Nodes (21): append_direct(), all_incomplete_append_prefixes_recover_to_the_last_complete_record(), complete_corrupt_record_and_invalid_header_are_rejected(), exact_legacy_header_prefixes_are_repaired_and_nonprefixes_fail_closed(), replay_deduplicates_identical_ids_and_rejects_conflicting_reuse(), writer_acknowledges_only_a_synced_transaction_and_reopens_it(), incomplete_or_rejected_records_cannot_advance_the_recovered_clock(), shared_clock_survives_tail_recovery_rotation_and_lower_tick_records() (+13 more)

### Community 372 - "coder.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 374 - "coder-fast.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 376 - "gameplay/admin.rs"
Cohesion: 0.20
Nodes (4): Admin, GIVE, SPAWN, TIME

### Community 377 - "config/tests.rs"
Cohesion: 0.39
Nodes (6): binding_conflicts_and_movement_keys_fail_back_to_defaults(), config_round_trips_through_explicit_path(), invalid_values_are_clamped_and_corrupt_files_fall_back(), profile_is_generated_once_and_survives_reload(), saving_sanitizes_public_values(), test_directory()

### Community 380 - "outbound/tests.rs"
Cohesion: 0.43
Nodes (6): aggregate_byte_limit_is_enforced_across_clients(), aggregate_high_water_mark_survives_sub_tick_queue_drain(), frame_admission_includes_the_frame_currently_being_written(), per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop(), pong(), shared_encoding_keeps_independent_byte_reservations_until_each_client_releases()

### Community 382 - "spawn.rs"
Cohesion: 0.49
Nodes (5): collides(), collides_cached(), request_missing(), spawn_position(), spawn_position_cached()

### Community 383 - "Registered anchored behavior"
Cohesion: 0.40
Nodes (5): Authority, scheduling and recovery, Bounds and limits, Public contract, Registered anchored behavior, Verification

### Community 386 - "fields_with_command"
Cohesion: 0.29
Nodes (5): block(), fields(), fields_with_command(), every_public_removal_cause_preserves_its_exact_luau_context(), triple()

### Community 387 - "EffectBuffer"
Cohesion: 0.23
Nodes (4): EffectBuffer, LIFE, MAX_EMBERS, FireStyle

### Community 389 - "render.rs"
Cohesion: 0.11
Nodes (8): Camera, DEPTH_FORMAT, MAX_PENDING_MESHES, SKY_COLOR, SUN_DIRECTION, UPLOAD_BYTES_PER_FRAME, UPLOAD_MESHES_PER_FRAME, VERTEX_FLOATS

### Community 390 - "seed"
Cohesion: 0.29
Nodes (8): entity(), hex_id(), partition(), present(), profile(), seed(), word(), words()

### Community 392 - "resolve_player_movement"
Cohesion: 0.42
Nodes (4): MAX_MOVEMENT_STEPS_PER_AXIS, player_collides(), resolve_player_movement(), ResolveError

### Community 393 - "avatars/tests.rs"
Cohesion: 0.28
Nodes (4): gpu_registered_player_palettes_preserve_default_and_color_all_three_parts(), HEIGHT, render(), WIDTH

### Community 394 - "register"
Cohesion: 0.31
Nodes (3): Chest, definition(), register()

### Community 396 - "plan_observed_request"
Cohesion: 0.33
Nodes (5): denied(), encode_command_arguments(), plan(), plan_observed_request(), plan_request()

### Community 398 - "decode"
Cohesion: 0.36
Nodes (3): decode(), encode(), property_identifier()

### Community 399 - "Luau runtime tools"
Cohesion: 0.40
Nodes (5): Deterministic randomness, Diagnostics, Examples and compatibility, Libraries, Luau runtime tools

### Community 402 - "declarer"
Cohesion: 0.11
Nodes (6): Declaration, declarer(), field(), parse_recipe(), Declaration, declarer()

### Community 406 - "notifications.rs"
Cohesion: 0.23
Nodes (4): Lane, MAX_EVENT_BYTES, QUEUE, view_entity()

### Community 407 - "render/effects/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_preview(), SHADER, verified_example_gpu_pass_survives_resize_and_grades_scene(), version_two_graph_composes_declared_inputs_and_parameters_on_gpu()

### Community 408 - "src/storage.rs"
Cohesion: 0.15
Nodes (11): CONTENT_MAP, CONVERSION_INCOMPLETE, FORMAT_VERSION, HEADER_LEN, MAGIC, MAX_SNAPSHOT_BYTES, SAVE_FORMAT_VERSION, TEMP_COUNTER (+3 more)

### Community 410 - "Public dynamic-entity surface"
Cohesion: 0.29
Nodes (7): Behavior and movement, Bounds and remaining surfaces, External proof: Copperling, Interaction and presentation, Public dynamic-entity surface, Registration and identity, Verification

### Community 411 - "script/tests.rs"
Cohesion: 0.54
Nodes (6): elapsed_deadline_is_reported_with_module_identity(), input(), instruction_and_source_limits_reject_bad_modules_without_poisoning_worker(), memory_limit_and_syntax_error_are_attributable(), module(), sandbox_excludes_native_io_and_attributes_errors()

### Community 413 - "Public storage lifecycle boundary"
Cohesion: 0.33
Nodes (6): Chest integration, Explicit limits / next work, Lifecycle contract, Package boundary, Public storage lifecycle boundary, Verification

### Community 414 - "combined/mixed.rs"
Cohesion: 0.33
Nodes (4): combined_mod_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS

### Community 415 - "Registered content and composition"
Cohesion: 0.33
Nodes (6): Bounds, atomicity and compatibility, Composition and deterministic resolution, Existing capabilities exposed, Integration additions, Registered content and composition, Verification and limits

### Community 416 - "Archived plans and audits"
Cohesion: 0.50
Nodes (4): Archived plans and audits, Foundation and interface, Modding, Scripting

### Community 418 - "gameplay"
Cohesion: 0.13
Nodes (4): accepts(), capture(), prepare(), apply()

### Community 420 - "host-api/src/content.rs"
Cohesion: 0.12
Nodes (16): Block, BlockState, DropAnimation, .BYTE_LEN, FaceTextures, Geometry, Material, Property (+8 more)

### Community 421 - "presentation/effects/tests.rs"
Cohesion: 0.38
Nodes (3): avatar(), colored_sparks_follow_the_same_bounded_session_lifetime(), embers_follow_only_presented_entities_expire_and_stay_bounded()

### Community 422 - "Registered actions and composed controls"
Cohesion: 0.40
Nodes (5): Bounded composition, Negotiated commands, Production path and authority, Registered actions and composed controls, Supported targets and effects

### Community 423 - "client/appearance.rs"
Cohesion: 0.39
Nodes (3): apply_environment(), invalid(), parse()

### Community 424 - "client/drops/tests.rs"
Cohesion: 0.43
Nodes (4): authored_motion_uses_server_age_and_continues_into_partial_pickup(), item(), moving_drop_blends_between_authoritative_positions(), sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion()

### Community 428 - "run"
Cohesion: 0.43
Nodes (3): main(), parse_tint(), run()

### Community 429 - "route_effects"
Cohesion: 0.31
Nodes (9): route_effects(), block_changes_fan_out_to_boundary_face_edge_and_corner_owners(), cell_effects_route_through_chunk_boundaries_and_euclidean_negative_coordinates(), chunk(), effects_for(), envelope(), local_and_cross_chunk_effects_share_the_interaction_commit_barrier_and_stable_order(), output_overflow_is_explicit_and_rejects_the_whole_producer_buffer() (+1 more)

### Community 430 - "custom/tests.rs"
Cohesion: 0.38
Nodes (4): gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry(), gpu_preview(), gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters(), GREEN

### Community 431 - "src/client/tests.rs"
Cohesion: 0.07
Nodes (18): block_edit_uses_selected_hotbar_block_and_hit_face(), confirmed_fire_visuals_expire_and_are_capped_and_distance_culled(), graphics_controls_apply_save_and_preserve_values_while_disabled(), lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently(), latest_edit_mesh_survives_a_superseded_kiln_relight_backlog(), mapped_server_item_and_replaceable_state_drive_placement_preview(), moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness(), moving_objects_sample_current_local_light_across_negative_chunk_seams() (+10 more)

### Community 433 - "EffectBuffer"
Cohesion: 0.57
Nodes (3): EffectBuffer, EffectBufferError, EffectEnvelope

### Community 434 - "tests/effects.rs"
Cohesion: 0.33
Nodes (4): bundle(), DESCRIPTOR, SHADER, verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures()

### Community 435 - "quad"
Cohesion: 0.43
Nodes (3): linear_color(), quad(), Session

### Community 436 - "write_frame"
Cohesion: 0.25
Nodes (3): bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix(), byte_budget_stops_turns_and_file_limit_does_not_write_overshoot(), write_frame()

### Community 442 - "coder-smart.md"
Cohesion: 0.33
Nodes (5): Handoff, Implementation standard, Shared tree and commits, Start and scope, Verification

### Community 443 - "Phase 8: examples, parity and integrated verification"
Cohesion: 0.33
Nodes (6): Authoring and runnable examples, Integrated behavior and responsiveness, Phase 8: examples, parity and integrated verification, Production parity audit, Reproduce verification, Visual inspection and rendering context

### Community 445 - "run_loop"
Cohesion: 0.35
Nodes (3): apply_motion(), run(), run_loop()

### Community 446 - "Agent guidance"
Cohesion: 0.50
Nodes (4): Agent guidance, Architecture and invariants, graphify, Verify graphics and performance

### Community 448 - "ObserverRegistration"
Cohesion: 0.12
Nodes (4): CommittedBlock, CommittedEntity, ObserverRegistration, Catalog

### Community 449 - "visibility.rs"
Cohesion: 0.09
Nodes (10): Atmosphere, smooth(), create_sky_pipeline(), sky_camera_data(), SKY_SHADER, sky_basis_tracks_camera_turns_in_world_space(), chunk_visible(), chunk_visible_padded() (+2 more)

### Community 450 - "EntityTypeId"
Cohesion: 0.33
Nodes (5): a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema(), DROP_TYPE, fake_neighbour(), KILN_TYPE, PLAYER_TYPE

### Community 452 - "metadata"
Cohesion: 0.47
Nodes (3): metadata(), observer_metadata_is_inert_bounded_and_rejects_nested_envelopes(), runtime_metadata_rejects_unknown_shapes_and_unbounded_counts_with_valid_digest()

### Community 453 - "actors/tests.rs"
Cohesion: 0.70
Nodes (4): avatar(), interpolation_moves_between_samples_and_freezes_without_extrapolation(), landing_animation_follows_delayed_ground_contact_and_is_visual_only(), teleport_despawn_and_reappearance_reset_history()

### Community 454 - "create_target_pipeline"
Cohesion: 0.20
Nodes (3): create_target_pipeline(), target_outline_vertices(), TARGET_SHADER

### Community 458 - "Modding: start here"
Cohesion: 0.50
Nodes (4): Historical design and audit, Modding: start here, Rust extension and host reference, Try authoring now

### Community 461 - "net/tests.rs"
Cohesion: 0.21
Nodes (7): client_commands_are_rejected_until_content_ready_matches(), complete_content_handshake(), join_cleanup_enqueues_one_leave_with_the_next_sequence(), local_server_shutdown_restores_authoritative_position_on_next_start(), nonblocking_listener_streams_and_recovers_a_wal_acked_edit_inner(), production_reactor_joins_and_commits_an_edit_over_real_tcp(), socket_join_reloads_inventory_after_coordinator_refresh()

### Community 462 - "prepare_recovery"
Cohesion: 0.35
Nodes (7): decode_anchor(), invalid(), prepare_recovery(), read(), replay(), save(), Snapshot

### Community 465 - "bloxgloom"
Cohesion: 1.00
Nodes (3): bloxgloom, bloxgloom-host-api, bloxgloom-lifecycle-fixture

### Community 504 - "Registered inventory machines"
Cohesion: 0.40
Nodes (5): Contract, Registered inventory machines, Remaining boundaries, Trying the fixture, Verification

### Community 505 - "crate"
Cohesion: 0.08
Nodes (9): FIRST, PROFILE, SECOND, THIRD, admit(), invoke(), profile_state(), deadline() (+1 more)

### Community 506 - "src/client.rs"
Cohesion: 0.11
Nodes (10): ActionTracker, FRAME, INCOMING_FRAME_BUDGET, Keys, MAX_CHUNKS, MAX_INCOMING_PER_FRAME, MAX_OUTSTANDING_ACTIONS, run_client() (+2 more)

### Community 507 - "palette/tests.rs"
Cohesion: 0.43
Nodes (4): full_palette_reuses_removed_entry_and_preserves_other_cells(), row_copy_matches_flat_storage_in_each_mode(), state(), uniform_and_local_palette_boundaries_round_trip()

### Community 509 - "EntityTypeRegistryBuilder"
Cohesion: 0.09
Nodes (10): register(), kiln_block_states(), kiln_payload(), kiln_state(), KilnHalf, register_entity_type(), register_entity_type_with_recipes(), compiled_states_cover_both_halves_facings_and_lit_emission() (+2 more)

### Community 513 - "time"
Cohesion: 0.08
Nodes (11): animate(), MAX_APPLY_JOBS_PER_BARRIER, MIN_OWNER_TASKS_PER_WORKER_GROUP, writer_loop(), spread_over_tcp(), effect_patches_count_emissions_and_expose_their_replacement(), foreign_payloads_have_no_replacement(), plain_patches_carry_no_emissions_and_keep_their_replacement() (+3 more)

### Community 514 - "PlayerState"
Cohesion: 0.42
Nodes (4): PlayerState, read(), valid_key(), write()

### Community 516 - "decode"
Cohesion: 0.31
Nodes (3): decode(), encode(), float()

### Community 519 - "validate"
Cohesion: 0.80
Nodes (3): dword(), validate(), word()

### Community 521 - "add_test_client"
Cohesion: 0.11
Nodes (33): Extension, edit(), external_storage_lifecycle_seam_restart_retries_and_exact_refunds(), external_storage_rejects_blocked_footprint_and_stale_placement_without_debit(), ids(), open(), registered_slot_permissions_are_enforced_by_server_even_for_forged_requests(), resident() (+25 more)

### Community 524 - "entity"
Cohesion: 0.25
Nodes (4): definition(), KEY, State, definition()

### Community 529 - "src/lifecycle/tests.rs"
Cohesion: 0.83
Nodes (3): storage(), storage_faces_default_to_all_cardinal_normals_and_restrict_explicitly(), storage_faces_reject_empty_duplicate_and_non_cardinal_lists()

### Community 531 - "Player lifecycle implementation"
Cohesion: 0.25
Nodes (8): Landed: committed observers, Landed: exact identity and action queries, Landed: lifecycle state and scheduling, Landed: local public state and client lifecycle, Landed: targeted notices and session kicks, Landed: typed player commands and client roster, Player lifecycle implementation, Remaining implementation

### Community 535 - "avatars/mesh.rs"
Cohesion: 0.52
Nodes (5): AvatarMesh, AvatarVertex, build(), emit_cuboid(), humanoid_mesh_has_bounded_closed_cuboids_and_face_details()

### Community 536 - "voxel_view.rs"
Cohesion: 0.10
Nodes (14): MAX_REPLANT_CELLS, OWNER_PROBES_PER_TICK, Replanter, air_chunk(), key(), MissingChunk, MovementError, player_collides() (+6 more)

### Community 537 - "SCRIPTING.md"
Cohesion: 0.10
Nodes (15): Chunk generation for native contributors, Luau authoring in VS Code, Validate, Authored UI widgets and current limits, Exact identities and time, Local Luau packages, Package shape, Try the UI example (+7 more)

### Community 545 - "passes"
Cohesion: 0.60
Nodes (3): after_dependencies_use_the_same_cycle_and_missing_checks(), inputs_override_order_and_invalid_graphs_keep_resource_identity(), passes()

## Knowledge Gaps
- **894 isolated node(s):** `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS`, `REQUEST_TAG`, `TERRAIN_REQUEST_TAG` (+889 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 3539 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **152 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `EntityError` connect `EntityError` to `EntityStore`, `actions/entity.rs`, `register`, `kiln_footprint`, `.advance`, `MobilePages`, `entities/types.rs`, `drops/entity.rs`, `Inventory`, `EntityIndexes`, `plan`, `State`, `drops/planning.rs`, `Adapter`, `VoxelView`, `ServerStartup`, `entities/container.rs`, `host-api/src/machine.rs`, `server/entities/tests.rs`, `Adapter`, `EntityTransferPolicy`, `register`, `World`, `Port<P>`, `navigation.rs`, `CommitAction`, `EntityDependencies`, `entities/checkpoint.rs`, `.inventory_policies`, `EntityTypeRegistryBuilder`?**
  _High betweenness centrality (0.059) - this node is a cross-community bridge._
- **Why does `Inventory` connect `Inventory` to `add_test_client`, `server/durable.rs`, `notifications.rs`, `WorldSnapshot`, `join_named_client`, `join`, `State`, `actions/workstation.rs`, `Adapter`, `VoxelView`, `slots`, `ClientApp`, `script_startup/gameplay.rs`, `script_startup/gameplay/inventory.rs`, `Adapter`, `EntityTransferPolicy`, `invoke`, `reactor.rs`, `InventoryStore`, `Port<P>`, `.compose_current_package_action_with_args`, `Connection`, `Stack`, `CommitAction`, `server.rs`, `crate`, `src/client.rs`?**
  _High betweenness centrality (0.045) - this node is a cross-community bridge._
- **Why does `Chunk` connect `Chunk` to `preview.rs`, `OwnerData`, `OwnerPatch`, `render/mesh.rs`, `world/generation.rs`, `protocol.rs`, `Network`, `voxel_view.rs`, `handler.rs`, `predict_player_movement`, `src/client/tests.rs`, `VoxelView`, `systems/world.rs`, `Replicas`, `lighting.rs`, `ClientApp`, `client/entities/tests.rs`, `World`, `sealed_neighborhood`, `PalettedBlocks`, `ChunkCache`, `PublicEntity`, `render/tests.rs`, `streaming/entities.rs`, `src/world.rs`, `process_movement_batch`?**
  _High betweenness centrality (0.040) - this node is a cross-community bridge._
- **What connects `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS` to the rest of the system?**
  _894 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `EntityError` be split into smaller, more focused modules?**
  _Cohesion score 0.04986324426677888 - nodes in this community are weakly interconnected._
- **Should `EntityStore` be split into smaller, more focused modules?**
  _Cohesion score 0.06593977154724819 - nodes in this community are weakly interconnected._
- **Should `super` be split into smaller, more focused modules?**
  _Cohesion score 0.020305480682839175 - nodes in this community are weakly interconnected._