# Graph Report - bloxgloom  (2026-10-01)

## Corpus Check
- 1071 files · ~1,754,273 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 49 file(s) not represented in the graph (top: .glb 14, .mesh 13, .wgsl 11)

## Summary
- 14544 nodes · 34414 edges · 790 communities (502 shown, 288 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 1913 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `11ab21f6`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Item
- EntityStore
- Port<P>
- super
- render_previews
- .drop
- world_to_chunk
- temp_save_dir
- SystemDescriptor
- OwnerData
- server/runtime/tests.rs
- Complete modding implementation proposal
- validate_spawn
- OwnerPatch
- scheduler.rs
- server/durable.rs
- view
- entities/types.rs
- SystemId
- Catalog
- PackageSnapshot
- Cell
- protocol.rs
- Network
- OutboundFrame
- StateKey
- WorldSnapshot
- MobileProbe
- receipts.rs
- record.rs
- handler.rs
- Handler
- ui/draw.rs
- PendingWakeStore
- server_state_with_startup
- EntityIndexes
- EntityError
- Error
- IntentDelivery
- SystemRuntime
- journal/rotation.rs
- state_for
- arc
- BlockActionContext
- drops/planning.rs
- src/entity.rs
- Renderer
- UiLayout
- server/fire/tests.rs
- VoxelView
- server/effects.rs
- ServerMessage
- bench.rs
- intent/tests.rs
- src/client.rs
- Replicas
- Tag
- ServerStartup
- script.rs
- Registrar
- client/entities/tests.rs
- server/drops/tests.rs
- parallel.rs
- perf/fixture.rs
- World
- io
- actions/entity.rs
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
- Diagnostics
- owner/tests.rs
- Effect
- Receiver
- Authored materials and effects
- CheckpointWriter
- RegisteredEffectError
- EntityCheckpointMirror
- Connection
- ClientHandle
- TickId
- Value
- CommitAction
- third_person.rs
- package/client.rs
- voxel_view.rs
- JoinApp
- durable/state.rs
- VisualAvatar
- streaming.rs
- UiFrame<'_>
- solver.rs
- terrain.rs
- visibility.rs
- ComponentMatch
- ChunkLoader
- Journal
- script_startup/system.rs
- Session
- world/tests.rs
- dispatch.rs
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
- Payload
- InventoryId
- VisualSession
- entity_recovery/tests.rs
- effects/registered.rs
- host-api/src/actions.rs
- startup/tests.rs
- Committed
- presentation.rs
- Chunk
- Buffer
- Session
- Registered anchored behavior
- drops/entity.rs
- MovingSpawn
- world/generation.rs
- publication/commit.rs
- owner_durable/tests.rs
- tests/anchored.rs
- HandlerRegistration
- InventoryScreen
- render/drops.rs
- Texture
- server.rs
- custom/shader.rs
- InventoryProbe
- PostProcess
- Decision
- reactor/codec.rs
- script_startup/creature.rs
- Growth foundation plan
- world/generation/tests.rs
- extension_lifecycle.rs
- entity_checkpoint/tests.rs
- render/effects.rs
- .finish
- view.rs
- drops/queries.rs
- entities/player.rs
- Adapter
- gameplay/decisions.rs
- server/movement.rs
- handles.rs
- Execution
- systems/world.rs
- Capture
- custom.rs
- init
- Scripting capabilities for mod developers
- Composition
- public/tests.rs
- ClientApp
- .bind_machine
- UiRenderer
- AnchoredBlockEntity
- ContentManifest
- EntityDefinition
- decode
- script_startup/gameplay.rs
- client/bundle.rs
- sealed_neighborhood
- Clock
- parallel/tests.rs
- Proposal: one coherent gameplay API
- Sender
- content
- Adapter
- script/gameplay.rs
- Runtime
- .compose_current_package_action_with_args
- ProfileCell
- players/lifecycle.rs
- Update
- client/startup.rs
- VisualFire
- render/tests.rs
- machine_component_tests.rs
- character_asset.rs
- server/perf.rs
- Budget
- server/registry.rs
- neighborhood.rs
- script_startup/gameplay/profile_state.rs
- CoordinatorContext
- integer
- MovingEntity
- EffectKindId
- wake.rs
- AvatarRenderer
- Bindings
- snapshots.rs
- Preparation
- prepare
- preview.rs
- Ignitions
- position_store.rs
- services.rs
- draw
- Context<'_>
- durable/checkpoint.rs
- bounded.rs
- script_startup/appearance.rs
- pipeline.rs
- plan
- entity_sleep.rs
- model.rs
- Execution foundation: next implementation slices
- server/appearance.rs
- inventory
- script_startup/bundle.rs
- FirePending
- script/generation.rs
- BlockEditCommand
- authored.rs
- src/composition.rs
- Patrol
- ClientBundle
- client/lifecycle/tests.rs
- parse
- Cross-cutting integration findings
- package/tests.rs
- ScriptAnchored
- entities/motion/tests.rs
- slot.rs
- tick/tests.rs
- host-api/src/machine.rs
- Adapter
- Resources
- Gpu
- PlayerSummary
- resolve_nodes
- FootprintCell
- system/intents.rs
- World
- client/world.rs
- registry/tests.rs
- Hit
- Imports
- OwnerApplyReceipt
- decode_transaction
- EventRealm
- package/manifest.rs
- Observations
- CharacterEditor
- Declarations
- System
- Scripting Gap Closure Plan
- ScriptMachine
- invalid
- Invalid
- files.rs
- entity
- StorageBlockEntity
- process_movement_batch
- server/checkpoint/tests.rs
- .show_status
- registered/tests.rs
- showcase.rs
- memory.rs
- ItemIcon
- Adapter
- validate_changes
- Harvest
- authored/bindings.rs
- Result
- position_store/tests.rs
- declarer
- Inputs
- CharacterRecipe
- script/capacity.rs
- EntityDependencies
- Downloads
- Replace
- SignalPost
- refund_stacks
- inventory/container.rs
- src/storage.rs
- BootstrapContract
- _
- EntityAdapter
- client_metadata.rs
- BundleIdentity
- raycast_blocks
- CodecProbe
- src/content.rs
- .spawn
- script_startup/gameplay/entities.rs
- render.rs
- declarer
- prepare
- common.rs
- complete_barrier
- FireAnimator
- .register_action
- queries/tests.rs
- Player
- MobileEntity
- 2. Player and lifecycle hooks
- invoke
- mlua
- script_startup/generation.rs
- server/gameplay/entities.rs
- tcp/report.rs
- streaming/entities.rs
- publication.rs
- src/actions/tests.rs
- Shared<T>
- declarations/budget.rs
- journal/recovery.rs
- player_services/tests.rs
- Bloxgloom interface plan
- EntityCodecError
- reaction_removal_tests.rs
- nearest_unsent
- transfer.rs
- commands.rs
- script_startup/player.rs
- owner_wave/tests.rs
- Inventory
- public_systems/tests.rs
- register
- Bloxgloom
- .frame
- drop_merge.rs
- colliders.rs
- time
- anchored_tests.rs
- .handle
- validate_spawn_volume
- test_convert.py
- clearance.rs
- decode
- widgets.rs
- Config
- .first_solid_top
- lifecycle-fixture/src/content.rs
- journal/tests.rs
- generation
- coder.md
- install
- coder-fast.md
- script_startup/machine.rs
- gameplay
- owner_lane
- .handle
- .draw_node
- outbound/tests.rs
- decode
- join_named_client
- GpuPass
- registry
- declarer
- fields_with_command
- PlayerDecision
- .plan
- EntityTransferPolicy
- anchored/tests.rs
- resolve_player_movement
- render_avatars
- generate-pressure.py
- MobilePages
- plan_observed_request
- Registry
- scheduling_tests.rs
- config/tests.rs
- run_perf_benchmark_async
- declarer
- render_previews_at
- Capture
- validate
- notifications.rs
- render/effects/tests.rs
- .new
- chunk_loader/tests.rs
- Public dynamic-entity surface
- .public_view
- coordinates
- Behavior
- client/admin.rs
- render
- Archived plans and audits
- deviceext
- .accept
- items.rs
- Candidate
- request_chunk
- Registered actions and composed controls
- .validate_player_selection
- plan_changes
- OwnerWorldView
- visual/tests.rs
- CharacterAsset
- run
- character_asset/tests.rs
- custom/tests.rs
- General anchored block entities
- join_lifecycle.rs
- aggregate.rs
- install
- quad
- write_frame
- script_startup/gameplay/inventory.rs
- .generate
- Catalog
- Control
- DroppedItem
- coder-smart.md
- Phase 8: examples, parity and integrated verification
- native.rs
- run_loop
- Agent guidance
- startup/moving/tests.rs
- ScriptError
- std
- Startup
- avatars/appearance.rs
- resources.rs
- ObserverRegistration
- package
- public_systems/motion/tests.rs
- recipe_browser.rs
- client/appearance.rs
- actors/tests.rs
- .rotate_using
- reviewer.md
- net/tests.rs
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
- kiln_latency.rs
- .register_inventory_screen
- pin_thrown_drop_pickup_survives_restart_with_inventory
- duration
- declarer
- install
- time.rs
- Command
- Option
- src/appearance.rs
- gameplay/admin.rs
- src/client/tests.rs
- world
- app
- InventoryWorkers
- require
- modding/README.md
- Larger Luau packages and independent simulation features
- client/observations/tests.rs
- Context<'_>
- server/movement/teleport.rs
- Player lifecycle implementation
- parse
- PlayerState
- cold.rs
- Input
- presentation/effects/tests.rs
- script_startup/machine/components.rs
- admin/tests.rs
- transfer
- server/drops.rs
- script/tests.rs
- Modding: start here
- durable/fire/tests.rs
- welcome/assets/fonts/FONT.md
- drop_pickup.rs
- visual_contracts.rs
- visit
- Typed client replica snapshots
- predict_player_movement_with_stance
- receipts/tests.rs
- Registered content and composition
- load.rs
- client/entities/kiln.rs
- storage/policy.rs
- Dynamic authored UI and input
- script_startup/moving.rs
- create_target_pipeline
- bundle_ui.rs
- ReplicationProbe
- src/motion.rs
- memory/tests.rs
- FireFrontier
- Atlas
- moving/lifecycle.rs
- Authoritative moving entities
- EffectBuffer
- render/camera.rs
- extension_tests.rs
- extension_system.rs
- CharacterRenderer
- package_load.rs
- script_startup/drop_policy.rs
- render_block_preview
- render_daylight_previews
- entities
- Authored player vertical slice
- world_time/tests.rs
- record
- catalog
- seed
- transfer/tests.rs
- presentation/observations/tests.rs
- LocalIndex
- AppearanceState
- face/README.md
- plan_motion
- Compiled
- src/storage/tests.rs
- entities/moving_tests.rs
- Appearance
- owner_codec.rs
- TransferSelection
- DropPolicy
- WorkstationView
- Resolved
- atomic
- menus/tests.rs
- state
- .from_builtin_parts
- mpsc
- declarer
- validate_sources
- Luau VM lifetime and module state
- decode
- set_preview_block
- OwnerEffectPatch
- stack
- vm_latency.rs
- Spawn
- resolve
- .player_lifecycles
- route_effects
- colliders/tests.rs
- engine/tests.rs
- Extension
- SpawnReceipt
- content/moving/tests.rs
- progress_rotation
- probe_request
- SCRIPTING.md
- SharedParts
- connection/tests.rs
- declarations/moving/tests.rs
- materials.rs
- session_ids.rs
- prepare
- PlayerOperation
- navigation.rs
- prepare
- EffectBuffer
- register
- decode
- WorldTime
- entities/motion.rs
- Cadence
- writer_loop
- .builtins
- falling/tests.rs
- src/motion/tests.rs
- Runtime
- .aimed_mobile
- present
- tests/effects.rs
- .owner_systems
- State
- prepare
- metadata
- session_ids/tests.rs
- replant.rs
- passes
- RequestError
- EffectLimits
- .recipe
- frozen_wake_registry
- .apply_synced_world_edits
- drop_stack
- validate
- render/camera/tests.rs
- target_sizes
- open
- STONE_ITEM

## God Nodes (most connected - your core abstractions)
1. `EntityError` - 235 edges
2. `SystemId` - 141 edges
3. `OwnerKey` - 140 edges
4. `StateKey` - 116 edges
5. `Value` - 109 edges
6. `TickId` - 103 edges
7. `ClientMessage` - 101 edges
8. `ScriptError` - 98 edges
9. `CommitAction` - 88 edges
10. `world_to_chunk()` - 86 edges

## Surprising Connections (you probably didn't know these)
- `F4 — New flammability meets a separate edit producer` --references--> `apply_synced_batch()`  [EXTRACTED]
  docs/archive/modding/MODDING-PARITY-AUDIT.md → src/server/durable/fire.rs
- `Validate` --references--> `require()`  [INFERRED]
  docs/modding/IDE.md → tools/character_assets/convert.py
- `Libraries` --references--> `require()`  [INFERRED]
  docs/modding/RUNTIME-TOOLS.md → tools/character_assets/convert.py
- `Runtime, delivery and save compatibility` --references--> `require()`  [INFERRED]
  SCRIPTING.md → tools/character_assets/convert.py
- `private_entity_overlay_does_not_authorize_the_next_decision_owner()` --calls--> `Probe`  [INFERRED]
  crates/host-api/src/gameplay/tests.rs → extensions/lifecycle-fixture/src/system.rs

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

## Communities (790 total, 288 thin omitted)

### Community 0 - "Item"
Cohesion: 0.07
Nodes (7): Components, Item, EntityPayloadCodec, EntityTypeDescriptor, EntityTypeRegistration, EntityTypeRegistryBuilder<'a>, TickPolicy

### Community 1 - "EntityStore"
Cohesion: 0.06
Nodes (39): encode_allocator_value(), encode_motion_value(), validate_ownership_mode(), allocator_state_key(), apply_operation_to_projection(), canonical_location(), cell_state_key(), chunk_state_key() (+31 more)

### Community 2 - "Port<P>"
Cohesion: 0.20
Nodes (4): block(), Interaction<P>, Port<P>, public_slots()

### Community 3 - "super"
Cohesion: 0.01
Nodes (17): abandoned_result_retires_transport(), completed_result_can_still_be_cancelled(), failure_and_retry_dispatch(), CYCLE_MS, INITIAL_MS, HAIR_MESHES, Codec, mixed_runtime_catalog_joins_restarts_and_remaps_saved_identities_without_execution() (+9 more)

### Community 4 - "render_previews"
Cohesion: 0.31
Nodes (23): render_avatar_preview(), render_character_motion(), render_character_preview(), render_character_styles(), render_chest_previews(), render_creature_preview(), render_drop_animation_previews(), render_drop_preview() (+15 more)

### Community 5 - ".drop"
Cohesion: 0.12
Nodes (37): drain_durable(), drop_active_len(), drop_nearby(), reside_neighbourhood(), run_empty_tick(), spawn_drop(), stage_entity_batch(), take_drop() (+29 more)

### Community 6 - "world_to_chunk"
Cohesion: 0.12
Nodes (44): public_spawn_and_self_removal_are_one_atomic_recoverable_transaction(), bin_catalog(), bin_count(), bin_startup(), BinPayload, BinPullTick, coordinator_drain_preserves_deferred_entity_tick_until_commit(), drive_poke_tick() (+36 more)

### Community 7 - "temp_save_dir"
Cohesion: 0.14
Nodes (39): unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers(), anchored_break_uses_registered_removal_without_duplicating_refunds(), automatic_pickup_rejects_uncredited_take_before_wal_admission(), committed_observers_see_public_results_only_after_receipt_without_replay(), generic_entity_spawn_update_remove_composes_with_breaks_and_recovers(), ObservingExtension, placement_decision_stages_neighbour_and_reward_once_across_seam(), registered_gameplay_combines_seam_edits_and_drops_and_recovers_once() (+31 more)

### Community 8 - "SystemDescriptor"
Cohesion: 0.12
Nodes (8): access(), depends_on(), IdentifierError, RegistryError, ResourceId, SystemDescriptor, SystemRegistry, validate_namespaced()

### Community 9 - "OwnerData"
Cohesion: 0.05
Nodes (13): OwnerCodec, OwnerData, Codec, config(), prepare(), prepare_writes(), OwnerCodecError, OwnerValueCodec (+5 more)

### Community 10 - "server/runtime/tests.rs"
Cohesion: 0.07
Nodes (38): captured_owner_reads_share_reservations_and_fence_exclusive_waves(), rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor(), mixed_active_and_recurring_due_owners_progress_with_and_without_wakes(), wake_advances_deadline_work_and_handler_can_return_to_active(), arbitrated_retry_withdraws_staged_wake_flags(), chunk_owner(), deferred_producer_waves_restage_their_durable_wakes(), dishonest_effect_accounting_is_rejected() (+30 more)

### Community 11 - "Complete modding implementation proposal"
Cohesion: 0.05
Nodes (39): 10. Custom models: explicitly deferred, 11. Developer workflow and maintenance rules, 12. Implementation order and deliverables, 13. Completion and verification, 14. Continuation record — update during implementation, 1. What approval means, 2. Product outcome, 3. Starting point: preserve the useful work (+31 more)

### Community 13 - "OwnerPatch"
Cohesion: 0.06
Nodes (15): MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_PATCH_BYTES_PER_JOB, MAX_OWNER_PATCH_WRITES_PER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, MAX_OWNER_WAVE_PATCH_WRITES, OwnerJob, OwnerJobError, OwnerPatch (+7 more)

### Community 14 - "scheduler.rs"
Cohesion: 0.08
Nodes (17): invalid(), FIRE_DELIVERY_SYSTEM_ID, FIRE_LANES, FIRE_SYSTEM_ID, FireCursor, FireLoadMetrics, FireRecovered, FireRuntime (+9 more)

### Community 15 - "server/durable.rs"
Cohesion: 0.06
Nodes (16): stage(), BlockDelta, CHECKPOINT_QUEUE_CAPACITY, CHECKPOINT_WORKERS, DirtyCheckpoint, Durability, FireCheckpointBatch, MAX_DEFERRED_DURABLE_ACTIONS (+8 more)

### Community 16 - "view"
Cohesion: 0.10
Nodes (18): Mossbun, View, BODY, gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling(), ground_motion_respects_walls_cliffs_seams_and_embedded_edits(), view(), BODY, register() (+10 more)

### Community 17 - "entities/types.rs"
Cohesion: 0.06
Nodes (19): validate_location_owner(), AnchorUpdate, CellCoord, EntityId, EntityLocation, EntityOwner, EntityOwnership, EntityPublicView (+11 more)

### Community 18 - "SystemId"
Cohesion: 0.06
Nodes (22): RegisteredEffectBuffer, RegisteredEffectOrderKey, ChunkKey, OwnerKey, OwnerPartition, SystemId, DurableOwnerStore, PreparedInsert (+14 more)

### Community 19 - "Catalog"
Cohesion: 0.10
Nodes (7): DropSize, Catalog, CHEST_ITEM, hash_bytes(), HOPPER_ITEM, ItemDef, KILN_ITEM

### Community 20 - "PackageSnapshot"
Cohesion: 0.05
Nodes (8): authored_drop_animation_negotiates_and_default_keeps_old_bundle(), drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle(), item_sprite_option_is_negotiated_and_omission_preserves_default_identity(), handler_declarer(), error(), Package, PackageSnapshot, declarer()

### Community 21 - "Cell"
Cohesion: 0.07
Nodes (9): Cell, Block, cell_random(), Context, Context<'a>, DropSpawn, Error, Plan (+1 more)

### Community 22 - "protocol.rs"
Cohesion: 0.11
Nodes (31): BLOCK_COUNT, Cursor, Cursor<'a>, frame(), invalid(), key(), MAX_ENTITY_INTERACT_BYTES, MAX_FIRE_BURSTS (+23 more)

### Community 23 - "Network"
Cohesion: 0.08
Nodes (13): ConfigWriter, connect_bundle_probe(), connect_catalog_probe(), connect_inventory_probe(), connect_ui_probe(), connect_visual_probe(), Incoming, Mesher (+5 more)

### Community 24 - "OutboundFrame"
Cohesion: 0.10
Nodes (11): ClientQueueTelemetry, OUTBOUND_AGGREGATE_BYTE_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundError, OutboundFrame, OutboundQueue, OutboundTelemetry (+3 more)

### Community 25 - "StateKey"
Cohesion: 0.17
Nodes (13): decode_envelope(), domain_tag(), encode_envelope(), filename(), FireCheckpointStore, is_interrupted_temporary(), MAX_FIRE_FILE_BYTES, MAX_FIRE_VALUE_BYTES (+5 more)

### Community 26 - "WorldSnapshot"
Cohesion: 0.07
Nodes (10): block(), combine_entities(), dispatch_neighbors(), error(), OperationInput, Participants, plan_removals(), plan_with_lifecycles() (+2 more)

### Community 27 - "MobileProbe"
Cohesion: 0.06
Nodes (10): MobileProbe, NetworkedVisualProbe, console_commands_reject_non_admin_and_recover_grant_over_nonblocking_listener(), creature_probe(), external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener(), mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart(), mixed_work(), response_samples() (+2 more)

### Community 28 - "receipts.rs"
Cohesion: 0.11
Nodes (18): Admission, checksum(), invalid(), MAGIC, MAX_PAYLOAD, MAX_REASON, MAX_SNAPSHOT, ReceiptEvent (+10 more)

### Community 29 - "record.rs"
Cohesion: 0.20
Nodes (16): Cursor, Cursor<'a>, ExpiryReason, Impact, invalid(), length(), Motion, Pending (+8 more)

### Community 30 - "handler.rs"
Cohesion: 0.14
Nodes (13): FireDeliveryHandler, FireDeliveryInput, FireDeliveryPatch, FireHandler, FireOwnerInput, FireOwnerPatch, MAX_DELIVERIES_PER_OWNER, MAX_DUE_CELLS_PER_OWNER (+5 more)

### Community 31 - "Handler"
Cohesion: 0.09
Nodes (19): Handler, Destroy, GroundRemoved, Neighbor, Removed, ChestBreak, CollectDrop, FlowerNeighbor (+11 more)

### Community 32 - "ui/draw.rs"
Cohesion: 0.10
Nodes (23): UiBuilder<'_>, UiBuilder<'_>, EDGE, FONT_HEIGHT, FONT_WIDTH, GOLD, inset(), item_color() (+15 more)

### Community 33 - "PendingWakeStore"
Cohesion: 0.08
Nodes (24): crc32(), decode_owner_wake_key(), decode_wake_value(), encode_wake_value(), invalid_data(), OWNER_WAKE_DOMAIN, owner_wake_key(), OWNER_WAKE_MAGIC (+16 more)

### Community 34 - "server_state_with_startup"
Cohesion: 0.11
Nodes (34): tick_once(), server_state_with_startup(), durable_counter_startup(), durable_pair_startup(), durable_twin_startup(), entity_and_owner_state_commit_as_one_atomic_record(), interrupted_multi_owner_wave_before_sync_recovers_whole_or_nothing(), interrupted_multi_owner_wave_recovers_atomically() (+26 more)

### Community 35 - "EntityIndexes"
Cohesion: 0.12
Nodes (14): _entity_cell_key_round_trip(), Bucket, ChunkPage, decode_cell_key(), decode_cell_owner(), decode_chunk_key(), encode_cell_key(), encode_cell_owner() (+6 more)

### Community 36 - "EntityError"
Cohesion: 0.04
Nodes (57): checked_body(), crc32(), Decoder, Decoder<'a>, Encoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION, ENTITY_CELL_VALUE_MAGIC (+49 more)

### Community 37 - "Error"
Cohesion: 0.08
Nodes (9): cached_profile_inventory_rechecks_each_handler_authority_and_latches_denial(), handler_random_is_stable_per_seed_cell_and_registration(), ignored_failures_cannot_publish_partial_operations(), Inventories, moving_spawn_references_are_local_and_never_predict_durable_ids(), private_entity_overlay_does_not_authorize_the_next_decision_owner(), staged_motion_rechecks_owner_coalesces_fields_and_rejects_caught_errors(), transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides() (+1 more)

### Community 38 - "IntentDelivery"
Cohesion: 0.09
Nodes (21): IntentDelivery, Mailbox, blocked(), decode(), decode_key(), encode(), is_key(), key() (+13 more)

### Community 39 - "SystemRuntime"
Cohesion: 0.07
Nodes (10): CommitBarrier, SystemRuntime, MAX_OWNER_VALUES_PER_SYSTEM, MAX_PENDING_OWNER_WAKES, PendingRegisteredWave, PreparedRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs (+2 more)

### Community 40 - "journal/rotation.rs"
Cohesion: 0.14
Nodes (38): crc32(), invalid_data(), Base, BASE_FORMAT_VERSION, BASE_FORMAT_VERSION_LEGACY, BASE_MAGIC, BASE_MAX_BYTES, base_path() (+30 more)

### Community 41 - "state_for"
Cohesion: 0.15
Nodes (25): profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default(), profile_appearance_save_failure_never_publishes_and_stops_mutation(), recipe_save_failure_never_publishes_and_invalid_selection_never_writes(), recipe_save_is_profile_bound_canonical_and_exposes_only_legacy_host_projection(), failed_startup_queue_does_not_register_a_ghost_profile(), full_outbound_queue_disconnects_only_the_slow_client(), joined_players_spawn_above_solid_terrain_with_headroom(), joins_find_lower_safe_surface_after_origin_support_is_mined() (+17 more)

### Community 42 - "arc"
Cohesion: 0.14
Nodes (6): Crush, KEY, MARKED_INPUT, REFINED_INPUT, register(), registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes()

### Community 43 - "BlockActionContext"
Cohesion: 0.06
Nodes (21): BlockActionContext, BlockActionHooks, BlockActionRegistry, BlockActionRegistryBuilder, BlockActionRegistryBuilder<'a>, BlockCommitBuilder, invoke_hook(), MAX_BLOCK_ACTION_HANDLERS (+13 more)

### Community 44 - "drops/planning.rs"
Cohesion: 0.13
Nodes (25): merge_target(), plan_error(), plan_expired(), plan_spawn_stack(), plan_spawns(), plan_spawns_with_extra(), plan_stack_spawns(), plan_stack_spawns_with_extra() (+17 more)

### Community 45 - "src/entity.rs"
Cohesion: 0.11
Nodes (13): Behavior, Body, Context, Cuboid, Error, Lifecycle, Movement, Neighbour (+5 more)

### Community 46 - "Renderer"
Cohesion: 0.04
Nodes (7): next_upload_index(), order_pending_mesh(), Renderer, RendererError, RenderStats, urgent_mesh_reorders_existing_pending_chunk_without_duplication(), create_depth()

### Community 47 - "UiLayout"
Cohesion: 0.10
Nodes (9): InventorySearch, search_rect(), join_action_rect(), centered_panel(), effective_ui_scale(), HitRect, UiLayout, UiControl (+1 more)

### Community 48 - "server/fire/tests.rs"
Cohesion: 0.12
Nodes (33): cursor_key(), frontier_key(), mailbox_key(), benchmark_frontier_bootstrap_precedes_first_live_fire_tick(), checkpoint_batch_replaces_one_complete_snapshot_and_applies_tombstones(), checkpoint_store_rejects_orphans_and_corruption_and_cleans_interrupted_temp(), chunk(), durable_lane_age_prioritizes_an_owner_deferred_by_wal_pressure() (+25 more)

### Community 49 - "VoxelView"
Cohesion: 0.04
Nodes (25): CapturedColumn, DropTickPlanner, PROBE, spawn_effects(), AnchorEchoTick, BadTick, BinExchange, CounterInteract (+17 more)

### Community 50 - "server/effects.rs"
Cohesion: 0.17
Nodes (12): block_change_owners(), CellCoord, Effect, EffectBatch, MAX_EFFECTS_PER_BATCH, MAX_EFFECTS_PER_OWNER, MAX_EFFECTS_PER_PRODUCER_TICK, owner_order() (+4 more)

### Community 51 - "ServerMessage"
Cohesion: 0.11
Nodes (29): read_server(), ServerMessage, action_receipts_round_trip_and_reject_invalid_ids(), admin_grant_wire_round_trips_and_rejects_invalid_counts(), catalog_with_many_states(), character_selection_is_session_scoped_and_bounded_on_wire(), client_messages_round_trip(), committed_action_spawn_mappings_roundtrip_and_reject_invalid_ordinals() (+21 more)

### Community 52 - "bench.rs"
Cohesion: 0.08
Nodes (17): FireApplyTimings, ACTIVE_CHUNKS, benchmark_cpu(), BenchSave, cell_index(), CHUNKS_X, CHUNKS_Z, FireCpuPhaseReport (+9 more)

### Community 53 - "intent/tests.rs"
Cohesion: 0.11
Nodes (33): due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility(), durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans(), durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation(), durable_intent_bootstrap_destination_conflict_retries_one_atomic_record(), durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required(), durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems(), cell(), chunk() (+25 more)

### Community 54 - "src/client.rs"
Cohesion: 0.09
Nodes (15): action_id(), command_action_id(), digit_slot(), edit_for_hit(), edit_for_hit_with_catalog(), FRAME, INCOMING_FRAME_BUDGET, Keys (+7 more)

### Community 55 - "Replicas"
Cohesion: 0.14
Nodes (12): Assembly, MAX_CHUNK_ENTITY_BYTES, MAX_CLIENT_ENTITY_BYTES, MAX_PENDING_BYTES, MAX_PENDING_COMMITS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot (+4 more)

### Community 56 - "Tag"
Cohesion: 0.21
Nodes (13): Block, BlockState, FaceTextures, Geometry, Material, Property, Tag, TagKind (+5 more)

### Community 57 - "ServerStartup"
Cohesion: 0.10
Nodes (6): entity_error(), internal_owner(), ServerStartup, ServerStartup, StartupEntityType, StartupOwnerCodec

### Community 58 - "script.rs"
Cohesion: 0.09
Nodes (11): Invocation, Limits, Output, Program, Request, run(), run_with(), ScriptFailure (+3 more)

### Community 59 - "Registrar"
Cohesion: 0.13
Nodes (3): CubeBlock, Registrar, RegistrationError

### Community 60 - "client/entities/tests.rs"
Cohesion: 0.12
Nodes (28): anchored_presentation_window_is_separate_scoped_and_sorted(), mossbun_adapter_uses_the_negotiated_catalog_assignment(), mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction(), presentation_entity_window_is_ordered_and_explicitly_bounded(), accept(), block_and_entity_changes_wait_for_whole_cross_chunk_commit(), checksum_conflict_and_revision_gap_request_resync_without_partial_install(), chunk() (+20 more)

### Community 61 - "server/drops/tests.rs"
Cohesion: 0.11
Nodes (30): active_len(), apply_expired(), assert_store_consistent(), drop_world(), drop_world_in(), DropWorld, insert_entry(), item() (+22 more)

### Community 62 - "parallel.rs"
Cohesion: 0.07
Nodes (26): BarrierError, BatchId, CancelError, CancellationToken, execute_task(), ExecutorConfigError, JobCompletion, JobKey (+18 more)

### Community 63 - "perf/fixture.rs"
Cohesion: 0.12
Nodes (20): ACTION_INTERVAL, add_clients_and_seed_drops(), DIRT_ITEM, DROP_HEIGHTS, install_chunks(), MAX_STEADY_TICKS, MIN_STEADY_TICKS, PLAYER_COUNT (+12 more)

### Community 64 - "World"
Cohesion: 0.09
Nodes (6): ChunkKey, ChunkReadStamp, EditBasis, LoadedChunk, PreparedEdit, World

### Community 65 - "io"
Cohesion: 0.06
Nodes (20): TURN_ENTRIES, WRITE_BYTES, MAX_FRONTIER_CELLS, CLOCK_DOMAIN, FILE_HEADER_LEN, FILE_MAGIC, FILE_VERSION, FRAME_OVERHEAD (+12 more)

### Community 67 - "actions/entity.rs"
Cohesion: 0.18
Nodes (18): capture_dependencies(), capture_entity_view_for_plan(), capture_tick_input(), capture_view_for_plan(), commit_tick_plan(), corrupt(), interaction_sight(), permission() (+10 more)

### Community 68 - "PalettedBlocks"
Cohesion: 0.13
Nodes (4): PalettedBlocks, PaletteView, set_palette_cell(), u16

### Community 69 - "server/entities/tests.rs"
Cohesion: 0.16
Nodes (23): decode_checkpoint(), encode_checkpoint(), a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema(), anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically(), checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types(), delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion(), DROP_TYPE, fake_neighbour() (+15 more)

### Community 70 - "conflict_tests.rs"
Cohesion: 0.14
Nodes (24): action(), coordinator_admits_two_independent_atomic_pickups_before_applying_either(), disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply(), drop_merge_absence_is_fenced_against_same_owner_motion_into_range(), hold(), neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders(), overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership(), plan() (+16 more)

### Community 71 - "GenerationError"
Cohesion: 0.08
Nodes (14): CHUNK_SIZE, Context, Contributor, GenerationError, in_world_bounds(), MAX_WRITES, mix(), Output (+6 more)

### Community 72 - "lifecycle-fixture/src/system.rs"
Cohesion: 0.14
Nodes (11): Clock, KEY, NeighborProbe, Pair, pair_definition(), Probe, WakeLoop, WakePair (+3 more)

### Community 73 - "perf/fire.rs"
Cohesion: 0.11
Nodes (22): ACTIVE_CHUNKS, CHUNKS_X, CHUNKS_Z, drain_durable(), ensure_resident(), fixture_action(), FOREST_BATCH_CHUNKS, forest_hash() (+14 more)

### Community 74 - "ChunkCache"
Cohesion: 0.11
Nodes (3): CacheEntry, ChunkCache, OwnerState

### Community 75 - "ClientMessage"
Cohesion: 0.05
Nodes (6): PackageActionProbe, PackageActionProbe, ClientMessage, fixture(), luau_player_appearance_is_authorized_rollback_safe_peer_replicated_and_saved(), Peer

### Community 76 - "PublicEntity"
Cohesion: 0.12
Nodes (27): BlockCellChange, enforce_frame_size(), EntitySnapshotPage, MAX_BLOCK_CHANGES_PER_PART, MAX_ENTITIES_PER_PAGE, MAX_ENTITY_CHANGES_PER_PART, MAX_ENTITY_SNAPSHOT_BYTES, MAX_ENTITY_SNAPSHOT_PAGES (+19 more)

### Community 77 - "invoke_fields"
Cohesion: 0.13
Nodes (15): cell_at(), checked(), entity_id(), invalid(), invoke_fields(), position_at(), state_bytes(), amount() (+7 more)

### Community 78 - "key"
Cohesion: 0.28
Nodes (14): animated_item_bundle_rejects_invalid_and_noncanonical_motion(), canonical_order_dependency_identity_and_count_bounds_are_verified(), decoder_rejects_server_classification_and_oversized_payloads_before_copying(), header(), key(), material_effect_and_ui_assets_coexist_in_canonical_bundle(), metadata_validates_namespace_capability_shape_and_limits_before_compilation(), package() (+6 more)

### Community 79 - "reactor.rs"
Cohesion: 0.12
Nodes (16): ACCEPT_BUDGET, has_admission_capacity(), has_admission_capacity_with_limit(), INVENTORY_WORKERS, IO_POLL_TIMEOUT, LISTENER_KEY, MAX_PENDING_LEAVES, READ_BUDGET (+8 more)

### Community 80 - "config.rs"
Cohesion: 0.13
Nodes (12): clamp_finite(), CONFIG_VERSION, MAX_FOV, MAX_SCALE, MAX_SENSITIVITY, MIN_FOV, MIN_SCALE, MIN_SENSITIVITY (+4 more)

### Community 81 - ".window_event"
Cohesion: 0.11
Nodes (5): escape_screen(), ClientApp, ClientApp, inventory_screen(), UiScreen

### Community 82 - "src/world.rs"
Cohesion: 0.08
Nodes (31): AIR, BEDROCK_Y, BLUE_FLOWER, CHUNK_SIZE, CHUNK_VOLUME, DIRT, FERN, GLOWSTONE (+23 more)

### Community 83 - "InventoryStore"
Cohesion: 0.13
Nodes (11): checksum(), HEADER_LEN, invalid(), InventoryStore, MAGIC, MAX_LEN, MIN_LEN, SLOT_FIXED_LEN (+3 more)

### Community 84 - "Change"
Cohesion: 0.06
Nodes (20): Durability, Durability, Change, arbitrate_key_sets(), build_owner_writes_parallel(), canonical_key_set(), OwnerCommit, OwnerWaveDurables (+12 more)

### Community 85 - "TickSample"
Cohesion: 0.11
Nodes (14): duration_nanos(), EVENT_LATENCY_STREAMS, LatencyEvent, LatencyRing, Metric, MetricsRecorder, MotionSample, nearest_rank() (+6 more)

### Community 86 - "tcp.rs"
Cohesion: 0.11
Nodes (15): drive(), exercise(), movement(), open_nuisance_peers(), PROFILE_BASE, run(), SEED, seed_inventories() (+7 more)

### Community 87 - "Pending"
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

### Community 92 - "Receiver"
Cohesion: 0.09
Nodes (8): Receiver, CommitReceipt, Journal, JournalWriter, Request, RotateError, RotationReceipt, WriterCommand

### Community 93 - "Authored materials and effects"
Cohesion: 0.40
Nodes (5): Authored materials and effects, Effect contract 2, Material contract 2, Preparation and compatibility, Typed values from Luau

### Community 94 - "CheckpointWriter"
Cohesion: 0.10
Nodes (7): checkpoint_shard(), checkpoint_worker(), CheckpointJob, CheckpointReceipt, CheckpointSubmitError, CheckpointWriter, panic_message()

### Community 95 - "RegisteredEffectError"
Cohesion: 0.19
Nodes (5): EffectConsumerOutput, ErasedEffectKind, RegisteredEffectError, TypedEffectKind<P, M, V, D, C>, PatchUsage

### Community 96 - "EntityCheckpointMirror"
Cohesion: 0.09
Nodes (11): CheckpointReceipt, CheckpointTicket, CheckpointWork, Command, EntityCheckpointMirror, Event, MAX_MIRROR_ADMISSIONS, MirrorMetrics (+3 more)

### Community 97 - "Connection"
Cohesion: 0.12
Nodes (8): JoinResponse, Connection, PendingWrite, PendingWriteKind, Phase, PendingLeave, JoinGuard, SimulationInput

### Community 98 - "ClientHandle"
Cohesion: 0.14
Nodes (9): ActionKind, active_slow_peer(), ClientHandle, ClientStats, handshake(), PendingAction, read_until_stop(), Ready (+1 more)

### Community 99 - "TickId"
Cohesion: 0.13
Nodes (13): ClockError, CommandQueue, CommandQueue<T>, DrainError, FIXED_STEP, FixedStepClock, OrderedCommand, OrderKey (+5 more)

### Community 100 - "Value"
Cohesion: 0.14
Nodes (18): Value, bytes(), decode(), delay(), component_schema(), drop_policy(), options(), boolean() (+10 more)

### Community 101 - "CommitAction"
Cohesion: 0.10
Nodes (29): CommitAction, batchable_motion(), cancel_prepared_entities(), command_action_id(), defer_action(), durable_request_profile(), fail_if_durability_failed(), fatal_stage_error() (+21 more)

### Community 102 - "third_person.rs"
Cohesion: 0.17
Nodes (8): avatar(), GameplayPose, prepare(), render_first_person_previews(), render_gameplay_animation_previews(), render_third_person_previews(), Shot, Perspective

### Community 103 - "package/client.rs"
Cohesion: 0.06
Nodes (31): ANIMATED_MAGIC, APPEARANCE_MAGIC, APPEARANCE_POLICY_MAGIC, BLOCK_OPTIONS_MAGIC, BLOCK_STATES_MAGIC, ClientSide, ClientSource, COMPONENTS_MAGIC (+23 more)

### Community 104 - "voxel_view.rs"
Cohesion: 0.11
Nodes (13): air_chunk(), key(), MissingChunk, MovementError, player_collides(), resolve_player_movement(), resolve_player_movement_with_body(), set_block() (+5 more)

### Community 105 - "JoinApp"
Cohesion: 0.06
Nodes (7): Attempt, Control, Prepared, Progress, Installing, JoinApp, JoinProgress

### Community 106 - "durable/state.rs"
Cohesion: 0.10
Nodes (12): stage(), touches_anchor(), open(), action_changes(), chunk_state_key(), decode_chunk_key(), decode_profile_key(), encode_action_receipt() (+4 more)

### Community 107 - "VisualAvatar"
Cohesion: 0.06
Nodes (16): ActorAnimator, DELAY, Track, Sample, STEP, Track, Motion, AvatarInstance (+8 more)

### Community 108 - "streaming.rs"
Cohesion: 0.15
Nodes (12): OutboundClientSnapshot, can_stream_snapshot(), can_stream_snapshot_size(), MAX_LOAD_RESULTS_PER_TICK, MAX_NEW_LOADS_PER_CLIENT, MAX_PREFETCH_CANDIDATES, poll_chunk_loads(), publish_streams() (+4 more)

### Community 109 - "UiFrame<'_>"
Cohesion: 0.13
Nodes (18): actions(), admin(), button(), draw(), EDGE, GOLD, MUTED, PANEL (+10 more)

### Community 110 - "solver.rs"
Cohesion: 0.23
Nodes (24): Blocked, Body, cap_speed(), check_bounds(), Collider, Contact, ContactMemory, ContactPolicy (+16 more)

### Community 111 - "terrain.rs"
Cohesion: 0.15
Nodes (31): Biome, collapse_surface(), Column, decorate_chunk(), generate_blocks(), generated_block(), generated_block_in_column(), generated_block_with_pattern() (+23 more)

### Community 112 - "visibility.rs"
Cohesion: 0.09
Nodes (10): Atmosphere, smooth(), create_sky_pipeline(), sky_camera_data(), SKY_SHADER, sky_basis_tracks_camera_turns_in_world_space(), chunk_visible(), chunk_visible_padded() (+2 more)

### Community 113 - "ComponentMatch"
Cohesion: 0.19
Nodes (7): ComponentMatch, ComponentOutput, ComponentValue, value_bytes(), exact(), input(), output()

### Community 114 - "ChunkLoader"
Cohesion: 0.14
Nodes (8): ChunkLoader, ChunkLoadResult, ChunkLoadTicket, Job, RequestStatus, stop_workers(), WORKER_COUNT, worker_loop()

### Community 115 - "Journal"
Cohesion: 0.08
Nodes (6): clock_key(), frame_len(), Journal, KnownRecord, SubmitError, Transaction

### Community 116 - "script_startup/system.rs"
Cohesion: 0.15
Nodes (21): commit(), Fixture, KEY, luau_burn_owner_uses_host_removal_semantics_and_persists_receipt(), luau_owner_after_dependencies_are_resolved_before_save_creation(), luau_owner_block_info_uses_captured_public_fields_and_restarts(), luau_owner_caught_invalid_entity_change_rejects_whole_wave(), luau_owner_drop_creation_shares_receipt_and_restarts() (+13 more)

### Community 118 - "world/tests.rs"
Cohesion: 0.12
Nodes (23): broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline(), chunk_cache_evicts_the_least_recently_used_resident(), composed_generation_is_ordered_and_does_not_change_builtin_baseline(), composed_generation_rejects_unknown_states_and_duplicate_keys(), corrupt_save_is_not_silently_discarded(), edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot(), edits_survive_restart_and_cache_eviction(), FixedGeneration (+15 more)

### Community 119 - "dispatch.rs"
Cohesion: 0.14
Nodes (15): apply(), disconnect(), prepare(), Prepared, publish(), effect(), fire_cue_is_dropped_for_backlogged_client_without_disconnect(), fire_cue_requires_subscription_and_follows_committed_world_frame() (+7 more)

### Community 120 - "PlayerRules"
Cohesion: 0.11
Nodes (9): Body, BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_RULES, BUILTIN_SPAWN, InvalidPlayerRules, MotionRates, PlayerRules (+1 more)

### Community 121 - "entities/checkpoint.rs"
Cohesion: 0.13
Nodes (13): CHECKPOINT_NAME, checkpoint_rejects_bad_magic_version_and_checksum(), checkpoint_write_read_is_atomic_and_bounded(), crash_left_temporary_checkpoint_fails_closed(), DIRECTORY_NAME, entity_invalid_data(), EntityCheckpointStore, invalid_data() (+5 more)

### Community 122 - "serve"
Cohesion: 0.04
Nodes (59): receive_content_manifest(), farming_scale_cold_and_cached_join_latency(), report(), luau_action_block_targets_keep_real_reach_sight_and_identity_checks(), luau_creature_replaces_itself_with_another_authored_type_over_real_listener(), mod_admin_grant_requires_server_identity_and_replays_once_over_listener(), mod_admin_spawn_and_drop_share_one_allocator_and_restart(), connect() (+51 more)

### Community 123 - "burn.rs"
Cohesion: 0.11
Nodes (11): assert_burned(), assert_uncommitted(), Burn, burn_startup(), BurnGrass, definition(), destination(), plant_burn() (+3 more)

### Community 126 - "HarvestSnapshot"
Cohesion: 0.14
Nodes (4): flower_harvests_itself_and_grass_and_leaves_have_distinct_loot(), harvest(), harvest_with_catalog(), HarvestSnapshot

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
Cohesion: 0.15
Nodes (9): BUNDLE_TIMEOUT, ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT, JoinCleanup, serve_client(), serve_listener(), serve_listener_with_stats() (+1 more)

### Community 132 - "ScriptSystem"
Cohesion: 0.11
Nodes (5): bytes(), declarer(), field(), ScriptSystem, table()

### Community 133 - "Plan: built-in/mod capability parity"
Cohesion: 0.08
Nodes (24): 1. Establish the boundary and parity inventory, 2. Complete the container/block-entity vertical slice, 3. Complete dynamic entities and presentation, 4. Close remaining gameplay and world surfaces, 5. Prove integration outside engine internals, Adopted direction, Client presentation and resources, Completed task: player-response path hardening (+16 more)

### Community 134 - "Payload"
Cohesion: 0.16
Nodes (6): Behavior, Context, interaction_request(), Reaction, RemovalCause, Payload

### Community 135 - "InventoryId"
Cohesion: 0.20
Nodes (7): Components, Context<'_>, InventoryId, PickupTransfer, PickupTransfer<'a>, Slot, Stack

### Community 136 - "VisualSession"
Cohesion: 0.13
Nodes (3): Script, PendingBatch, VisualSession

### Community 137 - "entity_recovery/tests.rs"
Cohesion: 0.17
Nodes (12): checkpointed_motion_ahead_of_wal_fence_survives_recovery(), fixture(), lagging_checkpoint_replays_later_wal_transfer(), MOBILE_TYPE, NEXT_DIR, one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt(), position(), same_revision_conflicting_checkpoint_motion_fails_closed() (+4 more)

### Community 138 - "effects/registered.rs"
Cohesion: 0.10
Nodes (15): EffectConsumerBatch, EffectConsumerScratch, MAX_EFFECT_BATCH_PAYLOAD_BYTES, MAX_EFFECT_BUFFER_PAYLOAD_BYTES, MAX_EFFECT_DESTINATIONS, MAX_EFFECT_KIND_ID_BYTES, MAX_EFFECT_KIND_PAYLOAD_BYTES, MAX_REGISTERED_EFFECT_KINDS (+7 more)

### Community 139 - "host-api/src/actions.rs"
Cohesion: 0.17
Nodes (14): Action, CommandPermission, MAX_ACTIONS, MAX_INTERACTION_PAYLOAD, MAX_REQUEST_ARGUMENTS, MAX_TARGET_ACTIONS, MAX_WIDGETS, Operation (+6 more)

### Community 140 - "startup/tests.rs"
Cohesion: 0.15
Nodes (19): block_auto_items_cannot_bypass_total_item_capacity(), caught_startup_execution_limit_cannot_publish_and_worker_recovers(), content_capacity_admits_full_block_item_texture_targets(), content_capacity_fixture(), content_capacity_max_plus_one_errors_survive_pcall_with_key_and_usage(), duplicate_keys_and_caught_declaration_errors_reject_all_startup(), Fixture, generator_installation_fixture() (+11 more)

### Community 141 - "Committed"
Cohesion: 0.18
Nodes (5): Committed, Observer, UseObserver, luau_committed_observer_timeout_cannot_block_receipt_or_later_observer_over_listener(), Witness

### Community 142 - "presentation.rs"
Cohesion: 0.17
Nodes (15): bounded_float(), Command, command_entity(), display_text(), EntityView, invalid(), optional_bounded_float(), Reply (+7 more)

### Community 143 - "Chunk"
Cohesion: 0.09
Nodes (20): build_bounce(), index(), is_opaque(), LightField, MAX_LIGHT, PLANE, propagate(), SIDE (+12 more)

### Community 144 - "Buffer"
Cohesion: 0.17
Nodes (4): Buffer, capture(), default_filter_preserves_structured_game_events_and_flushes_final_errors(), scoped_filters_and_invalid_filter_fallback_work_without_global_state()

### Community 146 - "Registered anchored behavior"
Cohesion: 0.40
Nodes (5): Authority, scheduling and recovery, Bounds and limits, Public contract, Registered anchored behavior, Verification

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

### Community 151 - "owner_durable/tests.rs"
Cohesion: 0.36
Nodes (15): byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation(), capacity_defers_while_corruption_stops(), chunk(), commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing(), committed_waves_mark_active_and_update_the_due_index(), counter_store(), dropped_prepared_wave_changes_nothing(), fed_deadlines_are_not_duplicated_and_replay_clears_stale_readiness() (+7 more)

### Community 152 - "tests/anchored.rs"
Cohesion: 0.14
Nodes (10): owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record(), owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal(), removed(), seed_storage(), storage_startup(), StorageNeighbor, StorageOwner, StorageRemoved (+2 more)

### Community 153 - "HandlerRegistration"
Cohesion: 0.14
Nodes (4): Event, EventKind, HandlerRegistration, RemovalCause

### Community 154 - "InventoryScreen"
Cohesion: 0.18
Nodes (6): InventoryScreen, MAX_SLOTS, MAX_STATUS_FIELDS, SlotGroup, StatusField, StatusFormat

### Community 155 - "render/drops.rs"
Cohesion: 0.14
Nodes (18): block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting(), DropMeshes, emit_cutout_drop(), flower_pickup_uses_cutout_crosses_instead_of_cube_faces(), grass_side_band_is_at_the_top_on_both_side_axes(), is_sprite_item(), MAX_CUTOUT_INDEX_BYTES, MAX_CUTOUT_VERTEX_BYTES (+10 more)

### Community 156 - "Texture"
Cohesion: 0.20
Nodes (5): Texture, Catalog, error(), texture_definition(), validate_display()

### Community 157 - "server.rs"
Cohesion: 0.05
Nodes (35): catalog_with_extension(), Client, DEFAULT_CLIENTS, DEFAULT_VIEW, EDIT_REACH, handle_message(), INPUT_CAPACITY, join_client() (+27 more)

### Community 158 - "custom/shader.rs"
Cohesion: 0.08
Nodes (11): validate(), compose(), STUBS, TYPES, validate(), validate(), compose(), STUBS (+3 more)

### Community 159 - "InventoryProbe"
Cohesion: 0.06
Nodes (16): InventoryProbe, external_item_action_composed_control_receipt_duplicate_stale_and_restart(), send(), send_with_session(), connect(), counter_total(), edit(), FIRST (+8 more)

### Community 161 - "Decision"
Cohesion: 0.13
Nodes (6): Behavior, Decision, Event, EventKind, Registration, State

### Community 162 - "reactor/codec.rs"
Cohesion: 0.12
Nodes (10): BundleHandshake, CodecWorkers, decode_worker(), DECODE_WORKERS, DecodeRequest, encode_worker(), ENCODE_WORKERS, EncodedFrame (+2 more)

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
Cohesion: 0.32
Nodes (8): Blocks, inventories, and processing, external_owner_world_read_survives_real_listener_join_and_restart(), external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp(), external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener(), registered_item_components_transfer_and_recover_over_real_listener(), send(), storage_roundtrip(), until()

### Community 167 - "entity_checkpoint/tests.rs"
Cohesion: 0.18
Nodes (13): admitted_event_permit_cannot_be_dropped_silently(), checkpoint_io_failure_closes_admission_and_reports_error(), delayed_payload_receipt_keeps_newer_checkpoint_only_motion(), fixture(), interrupted_stream_does_not_publish_and_worker_failure_releases_credit(), malformed_ordered_event_fails_closed_without_checkpoint_publication(), multi_turn_checkpoint_holds_generation_fence_without_a_live_capture(), NEXT_TEST_DIR (+5 more)

### Community 168 - "render/effects.rs"
Cohesion: 0.18
Nodes (10): Descriptor, MAX_DESCRIPTOR_BYTES, MAX_PASSES, MAX_SHADER_BYTES, Pass, prepare(), prepare_inner(), Prepared (+2 more)

### Community 170 - "view.rs"
Cohesion: 0.17
Nodes (14): SlotFilter, draw(), footer(), GOLD, header(), inventory(), machine(), MUTED (+6 more)

### Community 171 - "drops/queries.rs"
Cohesion: 0.21
Nodes (16): airborne_count(), capture_nearby(), collect_in_aabb(), distance_sq(), extractable(), has_expired(), live_drop(), LiveDrop (+8 more)

### Community 172 - "entities/player.rs"
Cohesion: 0.11
Nodes (10): MAX_PLAYER_ENTITY_PAYLOAD_BYTES, MAX_SESSION_PLAYER_ENTITIES, PLAYER_ENTITY_TYPE, player_public_payload_codec_is_fixed_size(), player_type_registers_only_against_catalogued_identity(), PlayerEntityPayload, PlayerEntityStore, PlayerPayloadCodec (+2 more)

### Community 173 - "Adapter"
Cohesion: 0.14
Nodes (4): Adapter, Lookups, MachinePayload, register()

### Community 174 - "gameplay/decisions.rs"
Cohesion: 0.12
Nodes (12): BLOCK_REGISTER, BLOCK_SOURCE, ByteState, ENTITY_REGISTER, ENTITY_SOURCE, entity_target_action_is_discovered_in_verified_session_catalog(), Fixture, luau_decisions_block_events_caught_error_rollback_and_restart() (+4 more)

### Community 175 - "server/movement.rs"
Cohesion: 0.09
Nodes (13): AckKind, credit_per_tick(), CREDIT_SCALE, FLOAT_ROUNDING_ALLOWANCE_PER_TICK, MAX_COMMANDS_PER_TICK, max_credit(), movement_cost(), MovementAck (+5 more)

### Community 176 - "handles.rs"
Cohesion: 0.15
Nodes (14): entity(), entity_value(), EntityId, identity_methods(), intern(), profile(), profile_value(), ProfileId (+6 more)

### Community 177 - "Execution"
Cohesion: 0.12
Nodes (9): Engine, isolated(), MAX_RESIDENT, NEXT_RUNTIME, Reservation, RESIDENT, Retained, RUNTIMES (+1 more)

### Community 178 - "systems/world.rs"
Cohesion: 0.25
Nodes (5): capture(), capture_entities(), EditInputs, plan_edits(), within_radius()

### Community 180 - "custom.rs"
Cohesion: 0.17
Nodes (12): compose(), Descriptor, Material, MaterialSource, MAX_DESCRIPTOR_BYTES, MAX_MATERIALS, MAX_SHADER_BYTES, prepare() (+4 more)

### Community 181 - "init"
Cohesion: 0.14
Nodes (3): DEFAULT_FILTER, filter(), init()

### Community 182 - "Scripting capabilities for mod developers"
Cohesion: 0.07
Nodes (32): module(), Actions, commands and gameplay decisions, Authored materials, effects and Luau parameters, Authoring tools and runnable examples, Client startup and authored UI, Committed server observations, Content, Durable owner systems (+24 more)

### Community 183 - "Composition"
Cohesion: 0.13
Nodes (9): Catalog, Composition, field(), MAX_MEMBERS, MAX_PACKAGES, MAX_TAGS, Package, tag_kind() (+1 more)

### Community 184 - "public/tests.rs"
Cohesion: 0.12
Nodes (15): BlockExtension, component_schema_package_and_tag_changes_are_compatibility_failures(), Contribute, Declare, Emitter, external_narrow_plant_uses_registered_selection_in_production_raycast(), fixture(), invalid_composition_and_missing_content_fail_atomically_before_installation() (+7 more)

### Community 185 - "ClientApp"
Cohesion: 0.10
Nodes (5): chunk_in_view(), ClientApp, lighting_depends_on(), mesh_priority(), LightSample

### Community 188 - "AnchoredBlockEntity"
Cohesion: 0.10
Nodes (3): AnchoredBlockEntity, Catalog, Catalog

### Community 189 - "ContentManifest"
Cohesion: 0.18
Nodes (10): checksum(), ContentEntry, ContentManifest, invalid(), MAGIC, MAX_ENTRIES, MAX_MANIFEST_BYTES, valid_state_key() (+2 more)

### Community 190 - "EntityDefinition"
Cohesion: 0.12
Nodes (3): EntityDefinition, EntityState, Catalog

### Community 191 - "decode"
Cohesion: 0.11
Nodes (8): simultaneous_verification_is_bounded_and_failure_releases_admission(), decode(), active_and_retiring_references_prevent_eviction_and_retry(), bundle(), metadata_corruption_after_pressure_does_not_restore_retired_memo(), ordinary_corruption_keeps_cache_and_retry_is_strictly_bounded(), pressure(), unused_cache_is_released_before_one_local_verification_retry()

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

### Community 199 - "content"
Cohesion: 0.08
Nodes (19): blend_opposite_pixels(), emission_strengths(), item_material_layer(), item_material_layer_for(), material_layer(), material_layer_for(), material_mips(), material_mips_for() (+11 more)

### Community 200 - "Adapter"
Cohesion: 0.16
Nodes (4): Adapter, error(), register(), spawn_clear()

### Community 201 - "script/gameplay.rs"
Cohesion: 0.12
Nodes (6): command_declaration(), command_schema(), Declaration, declarer(), registration(), ScriptHandler

### Community 202 - "Runtime"
Cohesion: 0.16
Nodes (4): count(), number(), own_key(), Runtime

### Community 203 - ".compose_current_package_action_with_args"
Cohesion: 0.22
Nodes (8): ActionChoice, compose_named_command(), compose_observed_entity_action(), compose_package_action(), compose_package_action_with_args(), PackageActionInput, authored_entity_action_uses_observed_identity_and_exact_bounded_arguments(), named_shortcut_is_inert_without_matching_session_command()

### Community 204 - "ProfileCell"
Cohesion: 0.18
Nodes (3): Context<'_>, ProfileCell, WorldSnapshot<'_>

### Community 205 - "players/lifecycle.rs"
Cohesion: 0.20
Nodes (10): committed(), drive(), enqueue(), Job, joined(), key(), leaving(), Published (+2 more)

### Community 206 - "Update"
Cohesion: 0.19
Nodes (9): decode(), defaults(), Definition, identifier(), Kind, MAX_PARAMETERS, State, Update (+1 more)

### Community 207 - "client/startup.rs"
Cohesion: 0.32
Nodes (9): ascii(), display(), execute(), execute_event(), execute_retained(), identity(), prepare(), run() (+1 more)

### Community 208 - "VisualFire"
Cohesion: 0.13
Nodes (8): FireRenderer, FLOATS, MAX_BYTES, MAX_FIRES, triangle(), vertices(), VERTICES_PER_FIRE, VisualFire

### Community 209 - "render/tests.rs"
Cohesion: 0.11
Nodes (8): mesh_chunk(), adjacent_leaves_skip_interior_cutout_faces(), grass_side_is_upright_on_both_wall_axes(), greedy_quads_repeat_material_once_per_voxel(), mapped_builtin(), meshing_uses_shared_chunk_layout_and_world_origin(), plants_have_two_crossed_cutout_quads_and_do_not_hide_ground(), remapped_connection_catalog_drives_foliage_meshes_and_drop_art()

### Community 210 - "machine_component_tests.rs"
Cohesion: 0.19
Nodes (9): exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers(), independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart(), load_neighbours(), payload(), public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports(), pulse(), SelectiveMachine, settle() (+1 more)

### Community 211 - "character_asset.rs"
Cohesion: 0.12
Nodes (16): BODY_PNG, Channel, ChannelPath, CLEAN_FACE_PNG, Clip, EYE_NAMES, EYE_PNGS, HAIR_BOUNDS (+8 more)

### Community 212 - "server/perf.rs"
Cohesion: 0.13
Nodes (13): drain_outbound(), DrainTotals, TempSaveDir, PHASE_NAMES, print_latency(), print_metric(), print_report(), run_fire_cpu_perf() (+5 more)

### Community 213 - "Budget"
Cohesion: 0.11
Nodes (11): Budget, decode(), MAGIC, wrap(), decode(), MAGIC, wrap(), cached_shared_artifacts_and_rejected_growth_release_exact_admission() (+3 more)

### Community 214 - "server/registry.rs"
Cohesion: 0.06
Nodes (22): AccessKind, BudgetKind, ExecutableSystem, F, MAX_DEPENDENCIES_PER_SYSTEM, MAX_NEIGHBOR_RADIUS, MAX_PHASE_EFFECTS_PER_TICK, MAX_PHASE_JOBS_PER_TICK (+14 more)

### Community 215 - "neighborhood.rs"
Cohesion: 0.16
Nodes (16): luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect(), luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes(), luau_neighborhood_radius_requires_exact_bounds_and_world_capability(), fixture(), GROW, id(), inbox(), load_neighborhoods() (+8 more)

### Community 216 - "script_startup/gameplay/profile_state.rs"
Cohesion: 0.29
Nodes (7): fixture(), lifecycle_profile_writes_compose_and_ambiguous_decisions_rollback_over_listener(), OFFLINE, profile_state_actions_are_atomic_owned_binary_and_restart_safe_over_listener(), profile_state_reads_reserve_existing_cells_and_absence_until_receipt(), profile_state_service_requires_package_capability_over_listener(), read_cell()

### Community 217 - "CoordinatorContext"
Cohesion: 0.21
Nodes (14): durable_actions(), fire_delivery(), fire_source(), input_authorization(), interaction_commit(), player_movement(), publish(), apply_simulation_input() (+6 more)

### Community 218 - "integer"
Cohesion: 0.11
Nodes (13): parse(), sequence_len(), slot_list(), Capabilities, captured_entity(), checked(), entity_identity(), invoke() (+5 more)

### Community 219 - "MovingEntity"
Cohesion: 0.20
Nodes (4): MovingEntity, Catalog, State, validate_motion()

### Community 220 - "EffectKindId"
Cohesion: 0.13
Nodes (5): EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen, EffectRegistryError, TypedEffectKind

### Community 221 - "wake.rs"
Cohesion: 0.15
Nodes (11): blocked(), canonical_wakes(), EntityWake, interact_producer(), INTERACT_PRODUCER_ID, MAX_WAKES_PER_PLAN, register_wake_kind(), route_wakes() (+3 more)

### Community 223 - "Bindings"
Cohesion: 0.23
Nodes (7): Action, allowed_key(), Bindings, letter(), NamedBindings, parse(), valid_action_key()

### Community 224 - "snapshots.rs"
Cohesion: 0.19
Nodes (9): apply(), dispatch(), finish(), MAX_SNAPSHOT_JOBS, Prepared, publish(), Selection, Target (+1 more)

### Community 225 - "Preparation"
Cohesion: 0.17
Nodes (3): Preparation, Ready, Renderer

### Community 226 - "prepare"
Cohesion: 0.18
Nodes (6): decode_motion_id(), invalid_data(), invalid_entity(), prepare(), PreparedEntityRecovery, validate_checkpointed_motion()

### Community 227 - "preview.rs"
Cohesion: 0.12
Nodes (9): CLIENT_MESH_RESULT_BATCH, CLIENT_PENDING_UPLOADS, FORMAT, MESHER_RESULT_CAPACITY, PERF_HEIGHT, PERF_RADIUS, PERF_STEADY_FRAMES, PERF_WIDTH (+1 more)

### Community 229 - "position_store.rs"
Cohesion: 0.19
Nodes (9): checksum(), invalid(), LEN, MAGIC, PositionStore, TEMP_SEQUENCE, validate_position(), validate_profile() (+1 more)

### Community 230 - "services.rs"
Cohesion: 0.19
Nodes (9): coordinate(), guarded(), integer_cell(), invoke(), LifecycleRequest, parse_lifecycle(), parse_spawn(), SpawnRequest (+1 more)

### Community 232 - "Context<'_>"
Cohesion: 0.31
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

### Community 237 - "plan"
Cohesion: 0.24
Nodes (5): capacity_error(), plan(), removal(), removal_cells(), removal_refunds()

### Community 238 - "entity_sleep.rs"
Cohesion: 0.30
Nodes (19): process_durable_actions(), queue_interaction_actions(), dispatch(), edit(), empty_action(), live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery(), new_sleepers_cannot_extend_the_current_recheck_pass(), position() (+11 more)

### Community 239 - "model.rs"
Cohesion: 0.04
Nodes (48): kiln_block_states(), kiln_footprint(), kiln_payload(), kiln_state(), FUEL_SLOT_INDEX, fuel_ticks(), INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS (+40 more)

### Community 240 - "Execution foundation: next implementation slices"
Cohesion: 0.12
Nodes (16): 1. Scheduling and progress under capacity pressure, 2. Reliable wake and sleep semantics, 3. Separate conflict revisions from publication ordering, 4. Consolidate commit orchestration and make barriers explicit, 5. Off-thread publication and bounded checkpoint work, Execution approach, Execution foundation: next implementation slices, First: review the current worker slice — done (+8 more)

### Community 241 - "server/appearance.rs"
Cohesion: 0.21
Nodes (7): checksum(), MAX_LEN, replace(), select(), select_character(), SEQUENCE, Store

### Community 242 - "inventory"
Cohesion: 0.05
Nodes (17): Codec, ContainerPayload, register(), register(), Slots, StoragePayload, StoragePayload<N>, CONTENT (+9 more)

### Community 243 - "script_startup/bundle.rs"
Cohesion: 0.24
Nodes (15): bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join(), bundle_restart_exact_cache_and_changed_source_require_new_bytes(), client_verification_rejects_relay_tamper_truncation_and_reordering(), closed(), fixture(), fragmented(), offer(), package_effect_is_prepared_by_real_client_join_before_welcome() (+7 more)

### Community 244 - "FirePending"
Cohesion: 0.12
Nodes (11): checked_body(), finish(), key_bytes(), read_key(), decode_pending_key(), FireIgnition, FireIgnitionId, FirePending (+3 more)

### Community 245 - "script/generation.rs"
Cohesion: 0.15
Nodes (7): coordinate(), Declaration, declarer(), invoke(), registration(), runtime(), ScriptContributor

### Community 246 - "BlockEditCommand"
Cohesion: 0.09
Nodes (15): block_intersects_player(), plan(), plan(), validate_player_credit(), BlockEditCommand, cached_block_or_request(), cached_block_with_reads(), ensure_no_unhandled_anchor() (+7 more)

### Community 247 - "authored.rs"
Cohesion: 0.12
Nodes (7): ATLAS_SIZE, Document, INVALID, json(), MAX_TEXT, Resources, Widget

### Community 248 - "src/composition.rs"
Cohesion: 0.10
Nodes (15): ACTIONS, ANCHORED_ENTITIES, Bundle, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, ITEM_ICONS (+7 more)

### Community 249 - "Patrol"
Cohesion: 0.17
Nodes (4): definition(), KEY, Patrol, State

### Community 250 - "ClientBundle"
Cohesion: 0.10
Nodes (10): CacheKey, ClientBundle, ClientPackage, command_metadata_roundtrips_schema_and_permissions_and_rejects_forgery(), generated_motion_handler_keys_use_native_runtime_key_vocabulary(), multiple_system_and_generator_metadata_roundtrips_and_rejects_excess_or_foreign_keys(), ordered(), Reader (+2 more)

### Community 251 - "client/lifecycle/tests.rs"
Cohesion: 0.26
Nodes (9): exercise_join_lifecycle(), exercise_player_services(), join(), player_notices_present_only_current_session_and_kicks_retire_it(), player_roster_accepts_newer_snapshots_and_is_cleared_when_session_retires(), public_player_snapshots_validate_session_and_services_then_clear_on_retirement(), read(), retired() (+1 more)

### Community 252 - "parse"
Cohesion: 0.13
Nodes (14): declarer(), field(), optional_integer(), optional_text(), owned(), parse(), state_key(), boolean() (+6 more)

### Community 253 - "Cross-cutting integration findings"
Cohesion: 0.12
Nodes (17): Approved unified gameplay implementation, Audit boundary and live path, Baseline and verdict, Built-in capability parity audit, Capability matrix, Cross-cutting integration findings, Definitions, assets, and composition, Evidence and integration acceptance (+9 more)

### Community 254 - "package/tests.rs"
Cohesion: 0.25
Nodes (11): cycles_depth_and_shared_execution_budget_are_bounded(), dependency_versions_and_manifest_declarations_are_strict(), discovery_bounds_directory_count_source_bytes_and_total_bytes(), discovery_rejects_symlinks_at_every_path_level_and_special_files(), failed_modules_are_not_reinitialized_when_caught(), Fixture, import_failures_name_package_version_and_module_and_do_not_poison_worker(), imported_source_limits_and_exported_function_errors_keep_source_identity() (+3 more)

### Community 256 - "entities/motion/tests.rs"
Cohesion: 0.36
Nodes (21): acceleration_and_capture_are_deterministic_across_chunk_seams(), body(), budgets_and_invalid_inputs_reject_instead_of_truncating(), corner_bounces_resolve_multiple_contacts_and_capacity_stops_safely(), dynamic_frame_bounce_obeys_declared_speed_cap(), earlier_wall_contact_prevents_false_world_boundary_removal(), embedded_contact_has_stable_unit_normal_without_pushout(), equal_time_contacts_have_stable_target_and_axis_order() (+13 more)

### Community 257 - "slot.rs"
Cohesion: 0.12
Nodes (6): draw(), draw(), color_from_swatch(), paint_icon(), show(), SlotStyle

### Community 258 - "tick/tests.rs"
Cohesion: 0.31
Nodes (10): drop_snapshot(), dropped_column_lands_at_rest_and_suspends(), falling_drop_integrates_exactly_one_fixed_step(), missing_terrain_defers_fail_closed_without_guessing(), neighbourhood_view(), neighbours(), NEXT_TEST_DIR, planner_rejects_anchored_locations_and_foreign_payloads() (+2 more)

### Community 259 - "host-api/src/machine.rs"
Cohesion: 0.22
Nodes (10): Context, FACES, Filter, Fuel, Machine, Port, Process, Recipe (+2 more)

### Community 262 - "Gpu"
Cohesion: 0.09
Nodes (9): AvatarMesh, AvatarVertex, build(), emit_cuboid(), humanoid_mesh_has_bounded_closed_cuboids_and_face_details(), Data, Gpu, MaterialData (+1 more)

### Community 263 - "PlayerSummary"
Cohesion: 0.21
Nodes (7): complete(), normalize(), token(), PlayerSummary, read(), validate_notice(), write()

### Community 264 - "resolve_nodes"
Cohesion: 0.14
Nodes (13): identifier(), owned(), bounded_nodes(), control(), dynamic_control_forest_validates_choices_ownership_and_container_parents(), Kind, Presentation, raw() (+5 more)

### Community 265 - "FootprintCell"
Cohesion: 0.12
Nodes (8): FootprintCell, footprint(), list(), observe(), offset(), parse(), parse(), parse()

### Community 266 - "system/intents.rs"
Cohesion: 0.18
Nodes (12): luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id(), luau_intent_send_requires_opt_in_and_absence_requires_bootstrap(), CHAIN, id(), inbox(), luau_intent_absent_destinations_run_on_real_listener_and_recover_once(), luau_intent_caught_invalid_and_overbudget_sends_poison_all_output(), luau_intent_declarations_and_session_identity_are_owned_and_bounded() (+4 more)

### Community 268 - "client/world.rs"
Cohesion: 0.22
Nodes (7): Cursor, MAX_PENDING_GROUPS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot, WorldProbe, WorldUpdate

### Community 269 - "registry/tests.rs"
Cohesion: 0.18
Nodes (19): builtin_phase_plan(), register_builtin_systems(), declarations_match_the_current_execution_shape(), descriptor(), deterministic_plan(), disjoint_and_read_only_accesses_can_share_a_phase(), duplicate_ids_and_invalid_namespaced_ids_are_rejected(), freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() (+11 more)

### Community 270 - "Hit"
Cohesion: 0.23
Nodes (14): probe_avatar(), probe_interact(), Face, Hit, is_plant(), make_hit(), MAX_REACH, nearest_face() (+6 more)

### Community 271 - "Imports"
Cohesion: 0.17
Nodes (3): Imports, MAX_IMPORT_DEPTH, ModuleState

### Community 272 - "OwnerApplyReceipt"
Cohesion: 0.22
Nodes (3): OwnerApplyReceipt, OwnerApplyTask, World

### Community 273 - "decode_transaction"
Cohesion: 0.20
Nodes (10): decode_transaction(), encode_frame(), frame_checksum(), invalid_data_owned(), invalid_input(), Reader, Reader<'a>, transaction_size() (+2 more)

### Community 274 - "EventRealm"
Cohesion: 0.17
Nodes (3): EventRealm, Export, load()

### Community 275 - "package/manifest.rs"
Cohesion: 0.18
Nodes (8): asset_path(), bounded_path(), identifier(), Manifest, public_path(), SourceSide, valid_path(), valid_version()

### Community 276 - "Observations"
Cohesion: 0.17
Nodes (11): ActionView, BlockView, ComponentView, InventoryView, key(), Observations, revision(), SlotView (+3 more)

### Community 279 - "System"
Cohesion: 0.10
Nodes (16): Behavior, DropSpawn, EntityChange, EntitySpawn, IntentId, IntentOutbox, IntentRequest, MAX_INTENT_PAYLOAD_BYTES (+8 more)

### Community 280 - "Scripting Gap Closure Plan"
Cohesion: 0.15
Nodes (13): Coverage and implementation order, Design requirements, Milestone 1 Authoring foundations, Milestone 2 Gameplay control and native parity, Milestone 3 World jobs and structured observations, Milestone 4 Dynamic UI, Milestone 5 Audio, Milestone 6 Models geometry and physics (+5 more)

### Community 282 - "invalid"
Cohesion: 0.11
Nodes (14): decode(), encode(), decode(), encode(), float(), decode(), encode(), float() (+6 more)

### Community 286 - "StorageBlockEntity"
Cohesion: 0.18
Nodes (10): MAX_FOOTPRINT, MAX_STORAGE_SLOTS, PlacementContext, PlaceStorage, RemovalContext, RemoveStorage, StorageBlockEntity, storage() (+2 more)

### Community 287 - "process_movement_batch"
Cohesion: 0.31
Nodes (18): MovementCommand, process_movement_batch(), air_chunk(), close(), command(), command_order_is_stable_and_affects_the_authoritative_position(), crouch_geometry_budget_and_unknown_standing_are_authoritative(), invalid_and_excessive_deltas_consume_sequence_without_changing_position() (+10 more)

### Community 288 - "server/checkpoint/tests.rs"
Cohesion: 0.27
Nodes (14): capacity_counts_running_jobs_and_unconsumed_receipts(), closure_panic_becomes_an_error_receipt_and_worker_keeps_running(), completed_but_unconsumed_receipt_still_occupies_capacity(), drop_drains_accepted_work_without_blocking_on_full_receipt_channel(), failed_write_is_returned_with_its_key_and_revision(), independent_checkpoint_keys_progress_while_another_shard_is_blocked(), key(), key_on_shard() (+6 more)

### Community 290 - "registered/tests.rs"
Cohesion: 0.39
Nodes (10): chunk(), emitted(), invalid_payload_and_duplicate_destinations_are_rejected(), keys(), oversized_expanded_fanout_aborts_routing_before_any_batch_is_returned(), registered_consumer_builds_a_typed_scratch_patch_for_its_destination(), registry(), routed_consumer_runs_once_with_stable_owner_kind_batch() (+2 more)

### Community 291 - "showcase.rs"
Cohesion: 0.10
Nodes (12): consume(), exercise_player_teleport(), phase4_showcase_creature_machine_and_replica_survive_real_join_and_restart(), PRESS_ANCHOR, send_action(), asset_aggregate_bytes_and_declaration_count_are_bounded(), asset_set_count_is_bounded_even_for_empty_files(), assets_use_secure_bounded_regular_file_reads() (+4 more)

### Community 292 - "memory.rs"
Cohesion: 0.32
Nodes (8): begin(), exceeded(), install(), memory_error(), memory_text(), observe(), reject(), Rejected

### Community 293 - "ItemIcon"
Cohesion: 0.14
Nodes (3): ItemIcon, definitions(), Catalog

### Community 294 - "Adapter"
Cohesion: 0.19
Nodes (3): Adapter, offset(), register()

### Community 296 - "Harvest"
Cohesion: 0.33
Nodes (3): Harvest, Pickup, PlantSupport

### Community 297 - "authored/bindings.rs"
Cohesion: 0.25
Nodes (6): Binding, Declaration, default_key(), resolve(), Scope, Session

### Community 299 - "position_store/tests.rs"
Cohesion: 0.21
Nodes (5): corrupted_position_is_not_silently_replaced(), position_checkpoint_does_not_share_inventory_temp_namespace(), position_round_trips_and_is_profile_scoped(), SEQUENCE, TestSave

### Community 300 - "declarer"
Cohesion: 0.24
Nodes (4): declarer(), field(), number(), triple()

### Community 301 - "Inputs"
Cohesion: 0.15
Nodes (3): expand(), Expansion, Inputs

### Community 303 - "script/capacity.rs"
Cohesion: 0.10
Nodes (21): BLOCKS_PER_PACKAGE, CLIENT_PREPARATION_WALL_TIME, GENERATION_SCRIPT_WALL_TIME, GENERATORS_PER_PACKAGE, INSTALLATION_WALL_TIME, ITEMS_PER_PACKAGE, MAX_ASSET_BYTES, MAX_ASSETS (+13 more)

### Community 304 - "EntityDependencies"
Cohesion: 0.16
Nodes (3): EntityDependencies, EntityStore, PreparedEntityTransaction

### Community 307 - "SignalPost"
Cohesion: 0.21
Nodes (3): KEY, SignalPost, State

### Community 308 - "refund_stacks"
Cohesion: 0.21
Nodes (4): place(), refund_removal(), refund_stacks(), remove()

### Community 309 - "inventory/container.rs"
Cohesion: 0.24
Nodes (5): decode(), encode(), invalid(), max_bytes(), independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data()

### Community 310 - "src/storage.rs"
Cohesion: 0.12
Nodes (20): checksum(), CONTENT_MAP, CONVERSION_INCOMPLETE, FORMAT_VERSION, HEADER_LEN, invalid_data(), MAGIC, MAX_SNAPSHOT_BYTES (+12 more)

### Community 312 - "_"
Cohesion: 0.15
Nodes (12): _, BYTES_PER_ROW, FORMAT, HEIGHT, MAX_MEASURED_FRAMES, run(), run_character_benchmark(), scene() (+4 more)

### Community 313 - "EntityAdapter"
Cohesion: 0.15
Nodes (3): Interaction, presentation and durability closure, kiln_adapter(), EntityAdapter

### Community 314 - "client_metadata.rs"
Cohesion: 0.24
Nodes (4): Catalog, Entity, Identity, Metadata

### Community 315 - "BundleIdentity"
Cohesion: 0.30
Nodes (8): BundleIdentity, CLIENT_RUNTIME_VERSION, MAX_BUNDLE_PART, read_identity(), read_part(), validate_part(), write_identity(), write_part()

### Community 316 - "raycast_blocks"
Cohesion: 0.22
Nodes (7): aiming_past_grass_edges_reaches_ground_but_center_hits_flower(), integer_plane_moving_negative_starts_in_the_entered_voxel(), parallel_axis_uses_half_open_boundary_ownership(), raycast_blocks(), reports_target_face_and_adjacent_cell(), simultaneous_corner_crossing_advances_all_axes(), traverses_negative_coordinates_and_negative_faces()

### Community 318 - "src/content.rs"
Cohesion: 0.05
Nodes (47): ACTIVE, block_flags(), BlockDef, BlockTextures, BUILTIN_EMISSION, BUILTIN_FLAGS, BUILTIN_REFLECTANCE, checked_id() (+39 more)

### Community 319 - ".spawn"
Cohesion: 0.14
Nodes (3): Lane, Runner, Snapshot

### Community 320 - "script_startup/gameplay/entities.rs"
Cohesion: 0.19
Nodes (7): Fixture, luau_entity_schema_loopback_spawn_due_callback_and_recovery(), luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling(), prepare(), REGISTER, REGISTER_ENTITY, SOURCE

### Community 321 - "render.rs"
Cohesion: 0.09
Nodes (12): avatar(), codec_normalization_tolerance_cannot_break_quaternion_interpolation(), committed_ticks_interpolate_orientation_without_gait_or_extrapolation(), impact_corrects_immediately_and_stale_motion_cannot_resurrect_flight(), model_replacement_and_removal_drop_retained_motion_history(), DEPTH_FORMAT, MAX_PENDING_MESHES, SKY_COLOR (+4 more)

### Community 323 - "prepare"
Cohesion: 0.29
Nodes (3): accepts(), capture(), prepare()

### Community 324 - "common.rs"
Cohesion: 0.12
Nodes (17): command_and_wait(), drop_candidates(), JoinedSnapshot, run_tick(), save_inventory(), Session, TEST_SAVE_SEQUENCE, TestSave (+9 more)

### Community 325 - "complete_barrier"
Cohesion: 0.19
Nodes (11): advance_receipts(), CommitProgress, complete_barrier(), drain_staged_receipts(), flush_ready_fire(), poll_journal_receipts(), caught_invalid_burn_removal_decision_rejects_whole_owner_wave(), luau_burn_removal_context_and_drop_commit_with_owner_receipt() (+3 more)

### Community 326 - "FireAnimator"
Cohesion: 0.23
Nodes (4): FireAnimator, LIFE, MAX_DISTANCE_SQUARED, MAX_FIRES

### Community 328 - "queries/tests.rs"
Cohesion: 0.33
Nodes (7): airborne_count_tracks_schedule_not_records(), expired_drop_is_visible_but_never_pickable(), immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation(), pickup_delay_gates_candidates_but_not_visibility(), single_drop_collection_rechecks_range_delay_and_expiry(), spawn_direct(), test_store()

### Community 329 - "Player"
Cohesion: 0.17
Nodes (4): Context<'_>, Player, capture(), publish_roster()

### Community 330 - "MobileEntity"
Cohesion: 0.17
Nodes (4): Animation, MobileEntity, Spawn, Catalog

### Community 331 - "2. Player and lifecycle hooks"
Cohesion: 0.05
Nodes (40): 1. Basic runtime tools — closed, 2. Player and lifecycle hooks, 3. Dynamic UI and input — closed within agreed scope, 4. General persistent block entities — closed, 5. Flexible entities, motion and presentation, 6. Development iteration and save continuity, 7. Content-pack scale and composition limits, Acceptance and examples (+32 more)

### Community 332 - "invoke"
Cohesion: 0.36
Nodes (3): admit(), invoke(), profile_state()

### Community 333 - "mlua"
Cohesion: 0.08
Nodes (4): declarer(), FixedBytes, declarer(), ScriptBehavior

### Community 334 - "script_startup/generation.rs"
Cohesion: 0.23
Nodes (9): Fixture, GENERATION, luau_generation_baseline_edits_and_identity_survive_restart(), luau_generation_rejects_bad_registration_and_caught_output_errors(), luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads(), luau_generation_streams_from_loader_after_restart(), MARKER, REGISTER (+1 more)

### Community 335 - "server/gameplay/entities.rs"
Cohesion: 0.41
Nodes (7): anchored(), nearby(), project(), read(), state(), validate_owner(), validate_state()

### Community 336 - "tcp/report.rs"
Cohesion: 0.31
Nodes (5): TransportSnapshot, OutboundSnapshot, percentile(), summarize(), TcpSoakReport

### Community 337 - "streaming/entities.rs"
Cohesion: 0.19
Nodes (5): MAX_PUBLIC_ENTITIES_PER_CHUNK, MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK, project(), snapshot_messages(), SnapshotError

### Community 338 - "publication.rs"
Cohesion: 0.33
Nodes (7): apply_committed_action(), apply_committed_action_inner(), apply_committed_fire_action(), apply_committed_owner_world(), is_owner_publication_key(), publish_committed(), publish_committed_fire_after_world()

### Community 339 - "src/actions/tests.rs"
Cohesion: 0.22
Nodes (6): action(), command_facets_are_empty_gameplay_only_and_fingerprint_permissions(), composed_controls_resolve_forward_references_without_changing_target_context(), composition_bounds_and_fingerprint_cover_every_control(), discovery_is_bounded_ordered_and_rejects_conflicting_ownership(), ordered_command_schema_has_canonical_bounded_arguments_and_identity()

### Community 341 - "declarations/budget.rs"
Cohesion: 0.25
Nodes (5): block_bytes(), MAX_BYTES, MAX_COUNT, package_bytes(), tag_bytes()

### Community 342 - "journal/recovery.rs"
Cohesion: 0.17
Nodes (8): Journal, Journal, legacy_header(), open_or_create_legacy(), sync_parent(), truncate_tail(), validate_legacy_header(), write_legacy_header()

### Community 343 - "player_services/tests.rs"
Cohesion: 0.23
Nodes (6): client_player_callback_rejection_is_atomic_and_other_packages_keep_running(), client_player_callbacks_compose_filter_public_state_and_disconnect_without_queue_room(), failed_client_import_initialization_is_cached_for_the_realm(), Fixture, retained_player_modules_keep_imports_and_coroutines_but_revoke_old_hosts(), states()

### Community 344 - "Bloxgloom interface plan"
Cohesion: 0.25
Nodes (8): Baseline when this plan was written, Bloxgloom interface plan, Delivery order, Goal, Interaction contract, Performance and correctness, UI and game-state design, Validation record

### Community 345 - "EntityCodecError"
Cohesion: 0.07
Nodes (14): BinCodec, CounterCodec, MateCodec, WideCodec, ByteCodec, Codec, decode_stack(), encode_payload() (+6 more)

### Community 346 - "reaction_removal_tests.rs"
Cohesion: 0.16
Nodes (6): drops(), open_soil(), RemovalDecision, SoilPost, SoilRegistration, stage_reaction()

### Community 347 - "nearest_unsent"
Cohesion: 0.29
Nodes (4): nearest_unsent(), boundaries_skip_unrepresentable_chunk_keys(), sent_keys_are_skipped_without_expanding_the_budget(), visits_every_interest_key_once_in_distance_order()

### Community 348 - "transfer.rs"
Cohesion: 0.18
Nodes (7): AutomationStack, EntityItemTransfer, movable_count(), move_up_to(), PortRoute, put(), take()

### Community 349 - "commands.rs"
Cohesion: 0.24
Nodes (7): advertised_builtin_commands_and_compatibility_packets_share_auth_receipts_and_restart(), command_request(), declaration(), invalid_command_declarations_poison_startup_even_when_caught(), negotiated_commands_enforce_permission_and_zero_args_with_receipts_and_restart(), typed_mod_command_negotiates_order_validates_before_handler_and_recovers_once(), typed_request()

### Community 350 - "script_startup/player.rs"
Cohesion: 0.26
Nodes (8): check_movement(), custom_player_rules_negotiate_before_welcome_and_survive_restart(), FIELDS, player_artifact_tampering_is_rejected_or_fails_exact_manifest_match(), player_rules_and_drop_animation_share_one_verified_bundle_before_join(), player_rules_compose_with_existing_sized_item_artifacts(), rejected_player_declarations_including_caught_errors_never_open_world(), source()

### Community 351 - "owner_wave/tests.rs"
Cohesion: 0.44
Nodes (9): a_system_wave_cannot_commit_two_patches_for_the_same_owner(), batch(), chunk(), handler_or_budget_failure_aborts_the_entire_wave(), patch(), results(), system(), validated_wave_applies_in_canonical_owner_order_after_aggregate_checks() (+1 more)

### Community 352 - "Inventory"
Cohesion: 0.05
Nodes (20): ComponentPayload, HOTBAR_SLOTS, Inventory, MAX_COMPONENT_BYTES, SLOTS, Stack, STACK_LIMIT, apply() (+12 more)

### Community 354 - "register"
Cohesion: 0.33
Nodes (3): definition(), KEY, register()

### Community 355 - "Bloxgloom"
Cohesion: 0.22
Nodes (9): Bloxgloom, Client execution and remaining extension work, Current execution architecture, Development and previews, HDR presentation, Publication and checkpoint boundaries, Run locally, Server threads and workers (+1 more)

### Community 356 - ".frame"
Cohesion: 0.15
Nodes (4): ClientApp, ClientApp, ClientApp, Camera

### Community 357 - "drop_merge.rs"
Cohesion: 0.24
Nodes (4): DropMergeCandidate, DropMergeContext, DropStackFill, filling_and_splitting_conserve_items_at_the_stack_cap()

### Community 358 - "colliders.rs"
Cohesion: 0.18
Nodes (8): capture(), collider(), History, intersects(), MAX_HISTORY_CREATURES, Pose, push(), sample()

### Community 359 - "time"
Cohesion: 0.11
Nodes (11): animate(), NEXT, acknowledged_result_stays_retired_across_rotation_and_restart(), grant(), inventory_action(), poll_until_settled(), temp_save_dir(), wal_replay_keeps_result_and_world_effect_before_checkpoint() (+3 more)

### Community 360 - "anchored_tests.rs"
Cohesion: 0.16
Nodes (14): anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic(), command(), edit(), fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record(), KEY, open(), public(), resident() (+6 more)

### Community 361 - ".handle"
Cohesion: 0.19
Nodes (3): DropStack, AnchorExtension, withdraw()

### Community 362 - "validate_spawn_volume"
Cohesion: 0.13
Nodes (4): prepare(), moving_launch_fences_complete_body_and_uses_final_terrain_overlay(), validate_spawn_volume(), WorldSnapshot<'_>

### Community 363 - "test_convert.py"
Cohesion: 0.09
Nodes (8): png(), convert(), geometry_animation_sha256(), Glb, load_native(), transform(), validate_geometry(), AssetTests

### Community 364 - "clearance.rs"
Cohesion: 0.19
Nodes (7): CharacterVertex, Box3, ClearanceFixture, HAIR_CUBOID_COUNTS, sampled_idle_walk_blends_keep_all_hair_clear_of_torso_and_limbs(), sampled_original_clips_keep_all_hair_clear_of_torso_and_limbs(), separating_margin()

### Community 365 - "decode"
Cohesion: 0.27
Nodes (3): decode(), encode(), Format

### Community 366 - "widgets.rs"
Cohesion: 0.32
Nodes (6): ControlValue, display_text(), nodes(), record(), sequence(), value()

### Community 367 - "Config"
Cohesion: 0.22
Nodes (4): Config, create_temporary_file(), parse_config(), write_and_replace()

### Community 368 - ".first_solid_top"
Cohesion: 0.42
Nodes (3): FallingContext, FallingPlan, FallingWorld

### Community 369 - "lifecycle-fixture/src/content.rs"
Cohesion: 0.20
Nodes (6): CHIP, Content, LAMP, PNG, REED, TEXTURE

### Community 370 - "journal/tests.rs"
Cohesion: 0.11
Nodes (18): append_direct(), all_incomplete_append_prefixes_recover_to_the_last_complete_record(), complete_corrupt_record_and_invalid_header_are_rejected(), exact_legacy_header_prefixes_are_repaired_and_nonprefixes_fail_closed(), replay_deduplicates_identical_ids_and_rejects_conflicting_reuse(), writer_acknowledges_only_a_synced_transaction_and_reopens_it(), incomplete_or_rejected_records_cannot_advance_the_recovered_clock(), shared_clock_survives_tail_recovery_rotation_and_lower_tick_records() (+10 more)

### Community 372 - "coder.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 373 - "install"
Cohesion: 0.23
Nodes (8): field(), install(), optional_vector(), parse_change(), parse_spawn(), SpawnRef, table(), vector()

### Community 374 - "coder-fast.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 375 - "script_startup/machine.rs"
Cohesion: 0.24
Nodes (13): luau_machine_footprint_negotiates_across_seam_and_restarts(), luau_machine_footprint_places_and_breaks_secondary_cell_over_listener(), luau_machine_negotiates_plans_and_restarts(), luau_machine_ports_and_transfer_work_negotiate_and_restart(), luau_machine_recipe_list_negotiates_filters_and_restarts(), luau_machine_recipe_list_rejects_overlapping_inputs_before_save(), luau_machine_rejects_invalid_ports_and_undeclared_transfer_work(), luau_machine_rejects_missing_capability_and_caught_invalid_recipe() (+5 more)

### Community 376 - "gameplay"
Cohesion: 0.11
Nodes (5): KEY, NEXT_REALM, install(), Output, SpawnRef

### Community 377 - "owner_lane"
Cohesion: 0.11
Nodes (6): cursor_lane(), FireRuntime, source_transaction(), SourceEncodeInput, FireRuntime, owner_lane()

### Community 379 - ".draw_node"
Cohesion: 0.24
Nodes (4): Intent, rgba(), Session, trim_bytes()

### Community 380 - "outbound/tests.rs"
Cohesion: 0.43
Nodes (6): aggregate_byte_limit_is_enforced_across_clients(), aggregate_high_water_mark_survives_sub_tick_queue_drain(), frame_admission_includes_the_frame_currently_being_written(), per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop(), pong(), shared_encoding_keeps_independent_byte_reservations_until_each_client_releases()

### Community 382 - "join_named_client"
Cohesion: 0.26
Nodes (6): join_named_client(), collides(), collides_cached(), request_missing(), spawn_position(), spawn_position_cached()

### Community 383 - "GpuPass"
Cohesion: 0.13
Nodes (3): Data, GpuPass, texture_entry()

### Community 384 - "registry"
Cohesion: 0.12
Nodes (5): no_hit(), no_interact(), player_adapter(), PLAYER_ENTITY_TYPE, project_avatar()

### Community 385 - "declarer"
Cohesion: 0.14
Nodes (3): identifier(), declarer(), Event

### Community 386 - "fields_with_command"
Cohesion: 0.26
Nodes (10): block(), fields(), fields_with_command(), motion(), motion_contact(), moving_target(), captured_motion_contact_preserves_exact_readonly_target_and_revision(), every_public_removal_cause_preserves_its_exact_luau_context() (+2 more)

### Community 390 - "anchored/tests.rs"
Cohesion: 0.35
Nodes (8): anchored_client_artifact_preserves_full_native_contract_and_catalog_identity(), anchored_client_artifact_rejects_storage_and_machine_ownership_collisions(), anchored_client_artifact_rejects_unresolved_refs_truncation_and_nested_wrappers(), artifact(), artifact_on(), base(), declaration(), decode_bytes()

### Community 392 - "resolve_player_movement"
Cohesion: 0.42
Nodes (4): MAX_MOVEMENT_STEPS_PER_AXIS, player_collides(), resolve_player_movement(), ResolveError

### Community 393 - "render_avatars"
Cohesion: 0.19
Nodes (11): catalog(), gpu_rigid_moving_model_rotates_in_three_dimensions_without_creature_deformation(), gpu_moving_projectile_flight_bounce_guidance_and_impact_filmstrip(), authored_gpu_character_draws_textured_animated_geometry_and_instance_tint(), different_recipes_color_only_selected_irises_and_swap_hair_per_instance(), gpu_registered_player_palettes_preserve_default_and_color_all_three_parts(), HEIGHT, render() (+3 more)

### Community 396 - "plan_observed_request"
Cohesion: 0.33
Nodes (5): denied(), encode_command_arguments(), plan(), plan_observed_request(), plan_request()

### Community 397 - "Registry"
Cohesion: 0.26
Nodes (3): key(), Registry, text()

### Community 398 - "scheduling_tests.rs"
Cohesion: 0.29
Nodes (6): increment(), invalid_handler_deadline_rejects_without_losing_due_work(), multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation(), run(), scheduled_startup(), wake_only_turns_without_runnable_work_preserve_ordinary_rotation()

### Community 400 - "config/tests.rs"
Cohesion: 0.31
Nodes (7): binding_conflicts_and_movement_keys_fail_back_to_defaults(), config_round_trips_through_explicit_path(), invalid_values_are_clamped_and_corrupt_files_fall_back(), named_shortcuts_round_trip_but_never_bind_movement_or_builtin_keys(), profile_is_generated_once_and_survives_reload(), saving_sanitizes_public_values(), test_directory()

### Community 401 - "run_perf_benchmark_async"
Cohesion: 0.21
Nodes (7): PerfGpuMesh, PerfGpuSubmesh, PerfPhase, PerfSample, print_percentiles(), run_perf_benchmark_async(), surface_height()

### Community 402 - "declarer"
Cohesion: 0.21
Nodes (4): Declaration, declarer(), field(), parse_recipe()

### Community 403 - "render_previews_at"
Cohesion: 0.23
Nodes (8): DropPhase, measure_ui_prepare(), preview_frame(), PreviewOutput, PreviewScene, render_previews_at(), render_previews_with_packages(), sample_inventory()

### Community 406 - "notifications.rs"
Cohesion: 0.23
Nodes (4): Lane, MAX_EVENT_BYTES, QUEUE, view_entity()

### Community 407 - "render/effects/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_preview(), SHADER, verified_example_gpu_pass_survives_resize_and_grades_scene(), version_two_graph_composes_declared_inputs_and_parameters_on_gpu()

### Community 408 - ".new"
Cohesion: 0.12
Nodes (4): array(), decode(), texture(), Targets

### Community 409 - "chunk_loader/tests.rs"
Cohesion: 0.31
Nodes (7): accepted_work_budget_has_explicit_nonblocking_overflow(), pre_edit_worker_result_is_rejected_after_uncheckpointed_edit(), receive_before(), requests_are_deduplicated_and_negative_chunks_load_asynchronously(), test_dir(), TEST_DIR_COUNTER, worker_load_uses_uncheckpointed_authoritative_snapshot_after_eviction()

### Community 410 - "Public dynamic-entity surface"
Cohesion: 0.07
Nodes (24): Behavior and movement, Bounds and remaining surfaces, External proof: Copperling, Interaction and presentation, Public dynamic-entity surface, Registration and identity, Verification, Chest integration (+16 more)

### Community 411 - ".public_view"
Cohesion: 0.35
Nodes (4): AppearanceCodec, encode_stack(), StackPayload, StackPayloadCodec

### Community 412 - "coordinates"
Cohesion: 0.23
Nodes (3): coordinates(), Event, Event<'a>

### Community 413 - "Behavior"
Cohesion: 0.22
Nodes (4): Behavior, DownwardFlow, Plan, Processor

### Community 414 - "client/admin.rs"
Cohesion: 0.18
Nodes (7): BINDING_ROWS_PER_PAGE, binding_targets(), BindingTarget, Command, parse(), parse_with_players(), registered()

### Community 415 - "render"
Cohesion: 0.45
Nodes (4): read_rgba_png(), render(), render_egui_previews(), render_package_egui_previews()

### Community 416 - "Archived plans and audits"
Cohesion: 0.50
Nodes (4): Archived plans and audits, Foundation and interface, Modding, Scripting

### Community 418 - ".accept"
Cohesion: 0.12
Nodes (5): ActionTracker, ClientApp, ClientApp, ClientApp, ClientApp

### Community 419 - "items.rs"
Cohesion: 0.30
Nodes (8): only_block_items_are_placeable(), placeable_block(), placeable_block_in(), SAPLING, SEEDS, STICK, valid_item(), valid_item_in()

### Community 421 - "request_chunk"
Cohesion: 0.15
Nodes (10): denied(), Invocation, plan(), read_target(), sight(), verify_reach(), is_registered(), plan() (+2 more)

### Community 422 - "Registered actions and composed controls"
Cohesion: 0.40
Nodes (5): Bounded composition, Negotiated commands, Production path and authority, Registered actions and composed controls, Supported targets and effects

### Community 425 - "OwnerWorldView"
Cohesion: 0.11
Nodes (6): MotionContact, Context, OwnedEntity, WorldRead, Adapter, OwnerWorldView

### Community 427 - "CharacterAsset"
Cohesion: 0.14
Nodes (7): CharacterAsset, blend(), CharacterAsset, overlay(), weight(), Joint, LocalPose

### Community 428 - "run"
Cohesion: 0.43
Nodes (3): main(), parse_tint(), run()

### Community 430 - "custom/tests.rs"
Cohesion: 0.38
Nodes (4): gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry(), gpu_preview(), gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters(), GREEN

### Community 431 - "General anchored block entities"
Cohesion: 0.40
Nodes (5): Callback, Declaration, Example and checks, General anchored block entities, Scheduling, authority and publication

### Community 433 - "aggregate.rs"
Cohesion: 0.20
Nodes (10): decode_snapshot(), encode_snapshot(), FILE_NAME, MAGIC, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_KEYS, read_snapshot_file(), TEMP_SEQUENCE (+2 more)

### Community 435 - "quad"
Cohesion: 0.43
Nodes (3): linear_color(), quad(), Session

### Community 436 - "write_frame"
Cohesion: 0.25
Nodes (3): bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix(), byte_budget_stops_turns_and_file_limit_does_not_write_overshoot(), write_frame()

### Community 437 - "script_startup/gameplay/inventory.rs"
Cohesion: 0.21
Nodes (10): actor(), luau_automatic_pickup_exact_components_conservation_and_restart(), luau_component_schema_rejects_wrong_payload_and_persists_exact_bytes(), luau_inventory_exact_binary_reads_give_take_and_error_rollback_restart(), luau_pickup_host_credit_eligibility_permissions_and_caught_errors_rollback(), luau_take_and_spawn_stack_preserve_binary_components_across_receipt_and_restart(), PICKUP, PICKUP_REGISTER (+2 more)

### Community 440 - "Control"
Cohesion: 0.25
Nodes (3): Control, control_values_preserve_byte_limits_ranges_and_declared_choices(), SelectOption

### Community 441 - "DroppedItem"
Cohesion: 0.17
Nodes (7): DropAnimation, .BYTE_LEN, DropAnimator, live_visual(), PickupFlight, POSITION_BLEND, DroppedItem

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

### Community 448 - "ScriptError"
Cohesion: 0.16
Nodes (13): decode_exact(), decode_input(), decode_output(), encode_exact(), encode_input(), encode_output(), BUDGET, exhausted() (+5 more)

### Community 449 - "std"
Cohesion: 0.20
Nodes (3): wal_reservation_accounts_for_queued_frames_before_the_worker_sees_them(), writer_treats_repeated_ids_idempotently_and_rejects_conflicts(), external_anchored_initialization_use_refund_and_restart_over_real_listener()

### Community 450 - "Startup"
Cohesion: 0.07
Nodes (3): ClientBundle, Format, Startup

### Community 452 - "resources.rs"
Cohesion: 0.19
Nodes (8): ArrayUsage, MAX_ARRAY_BYTES, MAX_ARRAY_LAYERS, required_limits(), counts_every_mipmap_and_enforces_the_array_byte_boundary(), requests_enough_device_layers_for_package_textures_and_native_materials(), target_package_texture_count_builds_a_valid_gpu_material_array(), validate()

### Community 453 - "ObserverRegistration"
Cohesion: 0.14
Nodes (4): CommittedBlock, CommittedEntity, ObserverRegistration, Catalog

### Community 454 - "package"
Cohesion: 0.25
Nodes (8): luau_anchored_callbacks_accept_full_registered_binary_limits_and_immutable_inputs(), luau_anchored_rejects_failed_validation_oversized_public_and_ambiguous_reaction(), luau_anchored_invalid_interaction_and_excess_refund_preserve_state_over_real_listener(), luau_anchored_registration_requires_capability_and_rejects_caught_invalid_geometry_before_save(), package(), sources(), luau_anchored_registration_rejects_unknown_sources_fields_and_out_of_range_bounds(), luau_anchored_then_storage_or_machine_same_block_rejects_caught_ownership_collision()

### Community 455 - "public_systems/motion/tests.rs"
Cohesion: 0.18
Nodes (5): Bytes, catalog(), MotionOwner, owner_moving_spawn_wraps_record_and_rejects_missing_authority_or_capture(), spawn()

### Community 456 - "recipe_browser.rs"
Cohesion: 0.29
Nodes (7): activate(), AIM, change(), open(), PROFILE, recipe_browser_dynamic_controls_real_server_crafting_rollback_replay_and_restart(), settle()

### Community 457 - "client/appearance.rs"
Cohesion: 0.39
Nodes (3): apply_environment(), invalid(), parse()

### Community 458 - "actors/tests.rs"
Cohesion: 0.42
Nodes (8): avatar(), fast_movement_cannot_accelerate_authored_walk_past_normal_playback(), interpolation_moves_between_samples_and_freezes_without_extrapolation(), landing_animation_follows_delayed_ground_contact_and_is_visual_only(), player_walk_blends_from_replicated_distance_then_stops_without_drift(), predicted_local_player_is_not_delayed_and_faces_the_current_look_heading(), switching_actor_model_cannot_reuse_character_gait_or_old_pose(), teleport_despawn_and_reappearance_reset_history()

### Community 461 - "net/tests.rs"
Cohesion: 0.15
Nodes (11): client_commands_are_rejected_until_content_ready_matches(), complete_content_handshake(), join_cleanup_enqueues_one_leave_with_the_next_sequence(), local_server_shutdown_restores_authoritative_position_on_next_start(), nonblocking_listener_streams_and_recovers_a_wal_acked_edit_inner(), production_reactor_joins_and_commits_an_edit_over_real_tcp(), connect(), crouch_loopback_cannot_stand_in_ceiling_and_late_join_observes_posture() (+3 more)

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

### Community 506 - "kiln_latency.rs"
Cohesion: 0.33
Nodes (8): action(), action_result(), chest_collects_hopper_output_while_moving_and_building_over_real_tcp(), observe(), package_downloads_and_cancellations_preserve_live_movement_edits_and_machine_progress(), placement_probe(), running_hopper_feeds_kiln_while_player_moves_and_places_over_real_tcp(), running_kiln_keeps_nearby_and_cross_chunk_placements_live()

### Community 508 - "pin_thrown_drop_pickup_survives_restart_with_inventory"
Cohesion: 0.67
Nodes (3): drop_totals(), pin_action_id(), pin_thrown_drop_pickup_survives_restart_with_inventory()

### Community 509 - "duration"
Cohesion: 0.10
Nodes (11): authored_motion_uses_server_age_and_continues_into_partial_pickup(), item(), moving_drop_blends_between_authoritative_positions(), sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion(), app(), avatar(), breaking_starts_a_bounded_tool_animation_and_stance_requires_server_confirmation(), held_break_cancels_on_menus_capture_loss_and_session_retirement() (+3 more)

### Community 511 - "install"
Cohesion: 0.14
Nodes (6): install(), present(), install(), latch(), number(), view()

### Community 513 - "Command"
Cohesion: 0.35
Nodes (4): Command, CommandArgument, CommandValue, MAX_COMMAND_ARGUMENTS

### Community 515 - "src/appearance.rs"
Cohesion: 0.33
Nodes (4): EYES, HAIR, MAX_APPEARANCE_BYTES, MOUTHS

### Community 516 - "gameplay/admin.rs"
Cohesion: 0.20
Nodes (4): Admin, GIVE, SPAWN, TIME

### Community 517 - "src/client/tests.rs"
Cohesion: 0.13
Nodes (9): block_edit_uses_selected_hotbar_block_and_hit_face(), confirmed_fire_visuals_expire_and_are_capped_and_distance_culled(), graphics_controls_apply_save_and_preserve_values_while_disabled(), lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently(), latest_edit_mesh_survives_a_superseded_kiln_relight_backlog(), mapped_server_item_and_replaceable_state_drive_placement_preview(), moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness(), moving_objects_sample_current_local_light_across_negative_chunk_seams() (+1 more)

### Community 518 - "world"
Cohesion: 0.11
Nodes (14): AIM, combined_mod_downloads_acts_grows_and_recovers(), combined_mod_two_profiles_act_independently_and_recover(), GROW, grow_once(), open(), PROFILE, farming_scale_downloads_plants_harvests_and_recovers_three_systems() (+6 more)

### Community 519 - "app"
Cohesion: 0.47
Nodes (3): app(), declared_input_real_client_queues_once_opens_and_rebinds_without_gameplay(), declared_input_real_client_respects_screens_focus_modifiers_and_scope()

### Community 521 - "InventoryWorkers"
Cohesion: 0.18
Nodes (3): inventory_worker(), InventoryLoadRequest, InventoryWorkers

### Community 522 - "require"
Cohesion: 0.21
Nodes (9): Luau authoring in VS Code, Validate, Deterministic randomness, Diagnostics, Examples and compatibility, Libraries, Luau runtime tools, Runtime, delivery and save compatibility (+1 more)

### Community 523 - "modding/README.md"
Cohesion: 0.16
Nodes (10): Growth foundation: remaining implementation, Chunk generation for native contributors, Luau contributor composition, Luau package composition, Public persistent owner systems, Documentation, Capacity pressure inputs, Farming package composition fixture (+2 more)

### Community 524 - "Larger Luau packages and independent simulation features"
Cohesion: 0.22
Nodes (9): Deterministic terrain contributors, Example and acceptance, Final package measurements, Graphics verification and controls, Independent systems, Larger Luau packages and independent simulation features, Memory and decoded-resource admission, Shared admission policy (+1 more)

### Community 525 - "client/observations/tests.rs"
Cohesion: 0.35
Nodes (8): app(), inventory_zero_is_known_and_stale_or_duplicate_updates_do_not_replace_it(), only_contiguous_installed_deltas_publish_authoritative_cells(), pending(), receipts_retain_package_ownership_deduplicate_and_retire_with_the_session(), recent_cells_are_bounded_refreshed_by_snapshots_and_removed_on_eviction(), terrain_action_receipts_preserve_the_registered_package_key(), ui_less_callbacks_receive_latest_world_snapshot_as_readonly_data()

### Community 529 - "server/movement/teleport.rs"
Cohesion: 0.32
Nodes (3): ready(), Reset, teleport()

### Community 531 - "Player lifecycle implementation"
Cohesion: 0.17
Nodes (12): Landed: committed observers, Landed: exact identity and action queries, Landed: general package-owned profile state, Landed: lifecycle state and scheduling, Landed: local public state and client lifecycle, Landed: runtime appearance, Landed: runtime teleport, Landed: targeted notices and session kicks (+4 more)

### Community 532 - "parse"
Cohesion: 0.36
Nodes (4): bytes(), invalid(), parse(), Reply

### Community 533 - "PlayerState"
Cohesion: 0.42
Nodes (4): PlayerState, read(), valid_key(), write()

### Community 535 - "cold.rs"
Cohesion: 0.29
Nodes (11): ACTION, BEHAVIOR, body(), cold_resume_fixture(), dormant_fixture(), launch_fixture(), moving_real_listener_cold_terrain_and_dormancy_preserve_then_resume_saved_motion(), open_boxed() (+3 more)

### Community 536 - "Input"
Cohesion: 0.24
Nodes (7): inside_view(), apply(), disconnect(), Input, prepare(), Prepared, reduce_view_under_pressure()

### Community 540 - "presentation/effects/tests.rs"
Cohesion: 0.38
Nodes (3): avatar(), colored_sparks_follow_the_same_bounded_session_lifetime(), embers_follow_only_presented_entities_expire_and_stay_bounded()

### Community 541 - "script_startup/machine/components.rs"
Cohesion: 0.16
Nodes (7): LifecyclePlan, Machine, luau_component_machine_processes_exact_stack_over_listener_and_recovers(), luau_machine_component_options_reject_invalid_constants_before_save(), luau_machine_component_recipe_negotiates_exact_predicate_and_preservation(), luau_machine_present_input_exact_output_and_component_fuel_roundtrip(), source()

### Community 542 - "admin/tests.rs"
Cohesion: 0.13
Nodes (4): generic_parser_uses_ordered_negotiated_schema_not_builtin_names(), player_command_names_completion_and_reconnect_use_exact_sessions(), request(), command_signature()

### Community 543 - "transfer"
Cohesion: 0.33
Nodes (3): Work, parse(), transfer()

### Community 544 - "server/drops.rs"
Cohesion: 0.12
Nodes (9): age_ms_now(), DROP_RADIUS, GRAVITY, invalid(), is_drop_delta(), LIFETIME, TERMINAL_SPEED, VIEW_RANGE (+1 more)

### Community 545 - "script/tests.rs"
Cohesion: 0.54
Nodes (6): elapsed_deadline_is_reported_with_module_identity(), input(), instruction_and_source_limits_reject_bad_modules_without_poisoning_worker(), memory_limit_and_syntax_error_are_attributable(), module(), sandbox_excludes_native_io_and_attributes_errors()

### Community 546 - "Modding: start here"
Cohesion: 0.50
Nodes (4): Historical design and audit, Modding: start here, Rust extension and host reference, Try authoring now

### Community 547 - "durable/fire/tests.rs"
Cohesion: 0.18
Nodes (14): apply_synced_batch(), Durability, run_delivery(), run_source(), stage_gameplay_burn(), stage_transactions(), stage_wave(), check_seeded_fire_restart() (+6 more)

### Community 552 - "visual_contracts.rs"
Cohesion: 0.33
Nodes (6): copy(), fixture(), installed_authoritative_entity_replica_drives_the_shader_parameter(), invalid_typed_startup_parameter_refuses_real_join_with_source_identity(), negotiated_visuals_parameters_switch_and_restart_without_global_state(), open_boxed()

### Community 555 - "Typed client replica snapshots"
Cohesion: 0.50
Nodes (4): Accepted implementation scope, Typed client replica snapshots, Update and lifecycle semantics, Verification

### Community 557 - "receipts/tests.rs"
Cohesion: 0.39
Nodes (5): ack_retires_both_outcomes_without_reopening_sequence(), bounded_window_can_run_beyond_old_lifetime_cap(), committed_launch_ids_survive_restart_and_duplicate_action_admission(), reconnect_closes_old_epoch_and_preserves_monotonic_epoch_after_codec(), record()

### Community 558 - "Registered content and composition"
Cohesion: 0.29
Nodes (7): Bounds, atomicity and compatibility, Composition and deterministic resolution, Existing capabilities exposed, Integration additions, Luau package capacities, Registered content and composition, Verification and limits

### Community 559 - "load.rs"
Cohesion: 0.15
Nodes (7): ACTION, count(), moving_real_listener_capacity_measurements(), package(), rank(), REGISTER, request()

### Community 560 - "client/entities/kiln.rs"
Cohesion: 0.19
Nodes (10): INSERT_FUEL, INSERT_INPUT, interact_verb(), interaction(), is_kiln_hit(), KilnCommand, no_avatar(), request_bytes() (+2 more)

### Community 562 - "Dynamic authored UI and input"
Cohesion: 0.40
Nodes (5): Callback inputs and atomic updates, Declared input actions, Documents and controls, Dynamic authored UI and input, Server authority and remaining scope

### Community 563 - "script_startup/moving.rs"
Cohesion: 0.18
Nodes (9): ACTION, BEHAVIOR, moving_failed_impact_remains_durable_and_does_not_grant_partial_reward(), moving_loopback_debits_once_sweeps_reacts_and_recovers_consumed_impact(), package(), prepare(), PROFILE, records() (+1 more)

### Community 564 - "create_target_pipeline"
Cohesion: 0.20
Nodes (3): create_target_pipeline(), target_outline_vertices(), TARGET_SHADER

### Community 566 - "bundle_ui.rs"
Cohesion: 0.13
Nodes (13): event(), authored_button_reaches_authoritative_receipt_and_durable_inventory(), client_startup_failure_refuses_content_ready_with_package_and_module(), downloaded_client_startup_is_session_scoped_across_reconnect_and_switch(), downloaded_replica_visuals_use_exact_entity_ids_and_reset_on_switch(), startup_fixture(), startup_worker_discards_partial_registration_and_caught_limit(), startup_worker_imports_exact_direct_dependencies_with_lexical_visibility() (+5 more)

### Community 568 - "src/motion.rs"
Cohesion: 0.16
Nodes (7): Body, CollisionMask, MAX_ACCELERATION, MAX_LIFETIME_TICKS, MAX_SOURCE_EXCLUSION_TICKS, MAX_SPEED, Response

### Community 569 - "memory/tests.rs"
Cohesion: 0.48
Nodes (5): memory_errors_latch_before_handlers_transform_them_and_reset_explicitly(), protected_calls_can_yield_and_resume_without_a_rust_boundary(), protected_calls_preserve_values_and_normal_errors(), real_allocator_error_caught_in_pcall_is_latched(), runtime()

### Community 577 - "moving/lifecycle.rs"
Cohesion: 0.24
Nodes (10): ACTION, BEHAVIOR, install(), launch(), moving_failed_expiry_keeps_profile_inventory_and_drop_rewards_atomic(), moving_real_listener_admin_cancel_rejects_nonadmin_and_clears_stuck_reaction(), moving_real_listener_dynamic_targets_source_exclusion_and_exact_receipt_replay(), moving_real_listener_expiry_effect_is_once_and_replay_preserves_historical_id() (+2 more)

### Community 578 - "Authoritative moving entities"
Cohesion: 0.14
Nodes (11): Acceptance evidence, Admission limits, Authoritative moving entities, Behavior events, Compatibility and presentation, Declaration, Gameplay services, Integration, reactions and persistence (+3 more)

### Community 579 - "EffectBuffer"
Cohesion: 0.23
Nodes (4): EffectBuffer, LIFE, MAX_EMBERS, FireStyle

### Community 580 - "render/camera.rs"
Cohesion: 0.29
Nodes (3): DISTANCE, intersection(), RADIUS

### Community 581 - "extension_tests.rs"
Cohesion: 0.31
Nodes (8): edit(), external_storage_lifecycle_seam_restart_retries_and_exact_refunds(), external_storage_rejects_blocked_footprint_and_stale_placement_without_debit(), ids(), open(), registered_slot_permissions_are_enforced_by_server_even_for_forged_requests(), resident(), startup()

### Community 582 - "extension_system.rs"
Cohesion: 0.15
Nodes (10): clock(), external_system_runs_without_entities_and_recovers_across_real_listener_restart(), startup(), luau_machine_active_state_negotiates_and_preserves_save_identity(), luau_machine_active_state_rejects_foreign_and_unpowered_states(), luau_machine_fuel_switches_authored_state_over_listener_and_recovers(), source(), luau_machine_variants_negotiate_place_from_finite_inventory_and_recover() (+2 more)

### Community 588 - "package_load.rs"
Cohesion: 0.28
Nodes (3): complete_package_handshake(), receive_package(), state_with_package()

### Community 592 - "entities"
Cohesion: 0.27
Nodes (6): live_command(), missing_mossbun_terrain_defers_locally_then_runs_on_residency(), mossbun_authorized_spawn_worker_steps_and_restart_preserve_identity(), mossbun_spawn_limit_rejects_without_allocating_or_consuming_items(), resident_platform(), pickup_and_entity_backlogs_leave_command_room_and_one_actor_cannot_fill_it()

### Community 593 - "Authored player vertical slice"
Cohesion: 0.09
Nodes (20): Additional head-look limitations, Attachment and preservation, Modular hair socket contract, Native regressions, Source-backed original-clip evidence, 128 actors, 300 measured frames (30 warmup), Avatar-free terrain sample, Complete 13-style catalog verification (+12 more)

### Community 594 - "world_time/tests.rs"
Cohesion: 0.39
Nodes (5): clock_command_recovers_past_an_older_checkpoint_and_rotated_wal(), local_listener_delivers_shared_world_time_admin_changes_and_recovers_it(), save(), temporary(), world_time_resumes_saved_phase_and_rejects_corrupt_state()

### Community 595 - "record"
Cohesion: 0.42
Nodes (6): apply_command(), declaration(), read(), read_contact(), record(), spawn_record()

### Community 597 - "seed"
Cohesion: 0.29
Nodes (8): entity(), hex_id(), partition(), present(), profile(), seed(), word(), words()

### Community 600 - "presentation/observations/tests.rs"
Cohesion: 0.60
Nodes (5): committed_spawn_ordinals_expose_exact_readonly_entity_handles(), known(), observations_distinguish_unknown_from_known_empty_and_filter_action_ownership(), observations_keep_exact_binary_revisions_and_nested_views_readonly(), observations_reject_invalid_dense_inventory_components_world_and_window_bounds()

### Community 602 - "AppearanceState"
Cohesion: 0.25
Nodes (3): AppearanceState, authored_recipes_replicate_independently_and_survive_server_restart(), Peer

### Community 605 - "Compiled"
Cohesion: 0.18
Nodes (3): Compiled, MAX_BYTES, MAX_ENTRIES

### Community 606 - "src/storage/tests.rs"
Cohesion: 0.29
Nodes (9): active_world_lock_excludes_a_second_writer_and_releases_on_drop(), content_map_preserves_wide_assignments_and_rejects_reassignment(), hidden_partial_conversion_stage_cannot_be_opened_even_after_marker_removal(), incomplete_conversion_cannot_be_opened_as_a_world(), old_world_is_rejected_without_creating_a_lock_or_rewriting_data(), temporary_root(), wide_sparse_edits_round_trip_and_reject_reordering(), with_extra_block() (+1 more)

### Community 607 - "entities/moving_tests.rs"
Cohesion: 0.24
Nodes (7): catalog(), control_only_publications_advance_full_motion_without_advancing_wire_position_revision(), motion_only_replica_commits_allow_envelope_changes_and_preserve_public_state_revision(), moving_projection_validates_redundant_wire_pose_and_exposes_only_authored_public_bytes(), projected(), State, unchanged_motion_revision_cannot_change_pose_or_other_envelope_fields()

### Community 608 - "Appearance"
Cohesion: 0.11
Nodes (8): Appearance, MAX_ADDITIONS, MODEL, PANTS, SHIRTS, SKINS, decode(), encode()

### Community 609 - "owner_codec.rs"
Cohesion: 0.08
Nodes (27): Entities, player behavior, world simulation and generation, advance_players(), crc32(), decode_cell_value(), decode_cursor_value(), decode_owner_cursor_key(), decode_owner_state_key(), encode_cell_value() (+19 more)

### Community 610 - "TransferSelection"
Cohesion: 0.18
Nodes (3): StackSelector, TransferSelection, Adapter

### Community 611 - "DropPolicy"
Cohesion: 0.20
Nodes (3): DropPolicy, .BYTE_LEN, .MAX_PICKUP_RANGE

### Community 614 - "atomic"
Cohesion: 0.08
Nodes (6): BUDGET, MAX_TRANSIENT_BYTES, Reservation, reserve(), AGE_BUCKETS, TransportStats

### Community 615 - "menus/tests.rs"
Cohesion: 0.40
Nodes (3): label_center(), native_character_menu_keeps_apply_visible_and_blocks_repeat_while_pending(), production_egui_graphics_exposes_and_labels_authored_character_control()

### Community 616 - "state"
Cohesion: 0.53
Nodes (4): classic_preview_preserves_palettes_and_clean_drafts_follow_server_changes(), draft_cancel_apply_echo_and_duplicate_apply_are_distinct(), state(), unknown_snapshot_and_rejected_draft_cannot_apply_and_disconnect_clears_state()

### Community 618 - "mpsc"
Cohesion: 0.13
Nodes (9): MAX_APPLY_JOBS_PER_BARRIER, MIN_OWNER_TASKS_PER_WORKER_GROUP, effect_patches_count_emissions_and_expose_their_replacement(), foreign_payloads_have_no_replacement(), plain_patches_carry_no_emissions_and_keep_their_replacement(), test_job(), test_owner(), WIDTH (+1 more)

### Community 623 - "Luau VM lifetime and module state"
Cohesion: 0.29
Nodes (7): Choose the right state, Contexts and coroutines, Failures and teardown, Initialization and random streams, Luau VM lifetime and module state, Runnable example and measurement, Verification

### Community 624 - "decode"
Cohesion: 0.33
Nodes (6): axes(), decode(), encode(), MAGIC, word(), wrap()

### Community 626 - "OwnerEffectPatch"
Cohesion: 0.09
Nodes (6): BlockEdit, EditCause, blocked(), EmittedOwnerEffect, OwnerEffectPatch, route_and_consume()

### Community 628 - "stack"
Cohesion: 0.31
Nodes (6): advance(), chest_hopper_chest_chain_preserves_last_slot_components_restart_and_refunds(), contents(), resident(), transfer(), apply()

### Community 629 - "vm_latency.rs"
Cohesion: 0.40
Nodes (3): CALLBACK, report(), vm_lifetime_mixed_listener_latency()

### Community 632 - "resolve"
Cohesion: 0.47
Nodes (3): broadcast(), clear(), resolve()

### Community 635 - "route_effects"
Cohesion: 0.36
Nodes (8): route_effects(), block_changes_fan_out_to_boundary_face_edge_and_corner_owners(), cell_effects_route_through_chunk_boundaries_and_euclidean_negative_coordinates(), chunk(), envelope(), local_and_cross_chunk_effects_share_the_interaction_commit_barrier_and_stable_order(), output_overflow_is_explicit_and_rejects_the_whole_producer_buffer(), route_rejects_mixed_ticks_duplicate_keys_and_excessive_limits()

### Community 636 - "colliders/tests.rs"
Cohesion: 0.25
Nodes (4): Bytes, declaration(), moving_capture_uses_committed_creature_crossing_and_fences_target_motion(), record()

### Community 638 - "Extension"
Cohesion: 0.10
Nodes (12): Extension, Fixture, KEY, TallStore, HarvestExtension, NoPickupExtension, PlacementExtension, SupportExtension (+4 more)

### Community 639 - "SpawnReceipt"
Cohesion: 0.44
Nodes (6): SpawnReceipt, MAX_ACTION_SPAWNS, read(), read_bytes(), validate(), write()

### Community 640 - "content/moving/tests.rs"
Cohesion: 0.33
Nodes (4): Bytes, declaration(), maximum_authored_state_with_long_pending_contact_fits_durable_envelope(), moving_catalog_bindings_follow_saved_identity_remapping()

### Community 643 - "SCRIPTING.md"
Cohesion: 0.09
Nodes (17): Authored UI widgets and current limits, Exact identities and time, Larger packages, Local Luau packages, Package shape, Try the UI example, Player rules, Phase 6 UI foundation (+9 more)

### Community 645 - "connection/tests.rs"
Cohesion: 0.36
Nodes (7): bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline(), commands_received_after_content_ready_wait_for_join_completion(), connected_streams(), frame_reader_keeps_partial_prefixes_until_the_payload_is_complete(), full_coordinator_queue_preserves_one_command_and_its_sequence(), pending_join_observes_eof_without_decoding_buffered_commands(), test_connection()

### Community 646 - "declarations/moving/tests.rs"
Cohesion: 0.36
Nodes (6): artifact(), base(), declaration(), decode_bytes(), moving_metadata_preserves_native_catalog_and_has_inert_codec(), moving_metadata_rejects_missing_capability_overflow_and_nesting()

### Community 647 - "materials.rs"
Cohesion: 0.29
Nodes (7): bound_texture_asset_must_decode_and_match_owned_canonical_metadata(), bundle(), DESCRIPTOR, material_bundle_verifies_key_ownership_limits_and_catalog_readiness(), SHADER, texture_metadata(), version_two_material_verifies_hooks_parameters_and_negotiated_layers()

### Community 745 - "session_ids.rs"
Cohesion: 0.24
Nodes (6): FILE, GENERATIONS, LOCK, next_after(), reserve(), TEMP_SEQUENCE

### Community 747 - "PlayerOperation"
Cohesion: 0.28
Nodes (3): PlayerOperation, PlayerOperationKind, apply()

### Community 748 - "navigation.rs"
Cohesion: 0.22
Nodes (4): MAX_NODES, RADIUS, Route, distance()

### Community 750 - "EffectBuffer"
Cohesion: 0.42
Nodes (4): boundary_coordinates(), EffectBuffer, EffectBufferError, EffectEnvelope

### Community 751 - "register"
Cohesion: 0.31
Nodes (3): Chest, definition(), register()

### Community 752 - "decode"
Cohesion: 0.36
Nodes (3): decode(), encode(), property_identifier()

### Community 755 - "entities/motion.rs"
Cohesion: 0.25
Nodes (7): DT, MAX_BODIES, MAX_CHUNK_BODIES, MAX_COLLIDERS, MAX_DYNAMIC_COLLIDERS, MAX_SWEEP_CELLS, STEP_TICKS

### Community 760 - "src/motion/tests.rs"
Cohesion: 0.52
Nodes (6): contact_query_tracks_captured_motion_revision_and_absence(), expiry_roundtrip(), pending_reaction_roundtrip_and_truncations(), public_projection_keeps_private_state_separate(), record(), reject_noncanonical_pose_and_impact_identity()

### Community 763 - "present"
Cohesion: 0.48
Nodes (3): entity(), present(), seed()

### Community 764 - "tests/effects.rs"
Cohesion: 0.33
Nodes (4): bundle(), DESCRIPTOR, SHADER, verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures()

### Community 768 - "metadata"
Cohesion: 0.47
Nodes (3): metadata(), observer_metadata_is_inert_bounded_and_rejects_nested_envelopes(), runtime_metadata_rejects_unknown_shapes_and_unbounded_counts_with_valid_digest()

### Community 769 - "session_ids/tests.rs"
Cohesion: 0.53
Nodes (4): directory(), session_boot_ranges_preserve_first_ids_and_never_reuse_a_durable_player_reference(), session_reservations_are_serialized_and_corrupt_or_exhausted_counters_are_rejected(), session_server_restart_installs_a_new_range_before_player_admission()

### Community 770 - "replant.rs"
Cohesion: 0.40
Nodes (3): MAX_REPLANT_CELLS, OWNER_PROBES_PER_TICK, Replanter

### Community 771 - "passes"
Cohesion: 0.60
Nodes (3): after_dependencies_use_the_same_cycle_and_missing_checks(), inputs_override_order_and_invalid_graphs_keep_resource_identity(), passes()

### Community 776 - "frozen_wake_registry"
Cohesion: 0.60
Nodes (5): frozen_wake_registry(), wake_buffer_overflow_rejects_the_whole_producer_output(), wake_emit_with_unknown_kind_is_rejected(), wake_routing_is_deterministic_across_repeated_runs(), tick_producer()

### Community 779 - "validate"
Cohesion: 0.80
Nodes (3): dword(), validate(), word()

### Community 780 - "render/camera/tests.rs"
Cohesion: 0.83
Nodes (3): eye(), perspectives_orbit_the_eye_and_cycle_without_changing_aim(), swept_camera_stops_before_walls_and_handles_close_or_unknown_cells()

## Knowledge Gaps
- **1056 isolated node(s):** `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS`, `REQUEST_TAG`, `TERRAIN_REQUEST_TAG` (+1051 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 4264 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **288 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Built-in capability parity audit` connect `Cross-cutting integration findings` to `modding/README.md`?**
  _High betweenness centrality (0.057) - this node is a cross-community bridge._
- **Why does `Audit boundary and live path` connect `Cross-cutting integration findings` to `ServerStartup`, `temp_save_dir`, `Registration`, `Replicas`?**
  _High betweenness centrality (0.045) - this node is a cross-community bridge._
- **Why does `ClientApp` connect `ClientApp` to `PlayerSummary`, `VisualSession`, `Chunk`, `Observations`, `CharacterEditor`, `PlayerState`, `Network`, `.show_status`, `.accept`, `UiLayout`, `src/client.rs`, `Replicas`, `DroppedItem`, `FireAnimator`, `.compose_current_package_action_with_args`, `ClientMessage`, `Update`, `.window_event`, `Inventory`, `.frame`, `third_person.rs`, `VisualAvatar`, `Config`, `.begin_window_install`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **What connects `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS` to the rest of the system?**
  _1056 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Item` be split into smaller, more focused modules?**
  _Cohesion score 0.07013574660633484 - nodes in this community are weakly interconnected._
- **Should `EntityStore` be split into smaller, more focused modules?**
  _Cohesion score 0.06467181467181467 - nodes in this community are weakly interconnected._
- **Should `super` be split into smaller, more focused modules?**
  _Cohesion score 0.014883628009189891 - nodes in this community are weakly interconnected._