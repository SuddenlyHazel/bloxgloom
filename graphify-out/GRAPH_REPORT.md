# Graph Report - bloxgloom  (2026-09-30)

## Corpus Check
- 722 files · ~1,598,885 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 17 file(s) not represented in the graph (top: .wgsl 10, .ttf 4, (none) 1)

## Summary
- 11816 nodes · 28938 edges · 516 communities (377 shown, 139 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 1665 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `67f42402`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- entities/persistence.rs
- EntityStore
- entities/types.rs
- super
- preview.rs
- state_for
- EntityTypeDescriptor
- world_to_chunk
- CommitAction
- OwnerData
- server/runtime/tests.rs
- src/content.rs
- VoxelView
- OwnerKey
- scheduler.rs
- server/durable.rs
- IntentOutbox
- OwnerEffectPatch
- SystemId
- src/storage.rs
- PackageSnapshot
- Cell
- protocol.rs
- Network
- OutboundFrame
- StateKey
- WorldSnapshot
- MobileProbe
- receipts.rs
- kiln/planning.rs
- handler.rs
- Handler
- ui/draw.rs
- PendingWakeStore
- server_state_with_startup
- collections
- EntityCodecError
- Inventories
- IntentDelivery
- SystemRuntime
- journal/rotation.rs
- join
- server.rs
- request_chunk
- drops/planning.rs
- MobileEntity
- Renderer
- UiLayout
- server/fire/tests.rs
- EntityError
- server/effects.rs
- EntityPublicView
- bench.rs
- intent/tests.rs
- ScriptError
- Replicas
- src/client/tests.rs
- ServerStartup
- script.rs
- Registrar
- client/entities/tests.rs
- server/drops/tests.rs
- PhaseExecutor
- perf/fixture.rs
- World
- stack
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
- invoke
- key
- Receiver
- config.rs
- .show_status
- src/world.rs
- Inventory
- Change
- TickSample
- tcp.rs
- Pending
- GameUi
- actions/entity.rs
- owner/tests.rs
- Effect
- JournalWriter
- modding/README.md
- CheckpointWriter
- src/client.rs
- EntityCheckpointMirror
- Connection
- ClientHandle
- TickId
- Attempt
- durable/coordinator.rs
- serve
- package/client.rs
- World
- JoinApp
- state.rs
- Adapter
- dispatch.rs
- menus.rs
- kiln/tests.rs
- terrain.rs
- BatchId
- ComponentMatch
- ChunkLoader
- Journal
- script_startup/system.rs
- Session
- world/tests.rs
- temp_save_dir
- PlayerRules
- entities/checkpoint.rs
- plan_durable_request
- Burn
- TerrainReads
- Registration
- HarvestSnapshot
- snapshots/tests.rs
- authored/tests.rs
- ScriptCreature
- content
- net.rs
- ScriptSystem
- host-api/src/system.rs
- Payload
- InventoryId
- VisualSession
- entity_recovery/tests.rs
- RegisteredEffectError
- host-api/src/actions.rs
- Appearance
- ObserverRegistration
- presentation.rs
- render/mesh.rs
- Buffer
- Session
- public/tests.rs
- drops/entity.rs
- reaction_removal_tests.rs
- world/generation.rs
- publication/commit.rs
- .activate_control
- StorageOwner
- HandlerRegistration
- InventoryScreen
- render/drops.rs
- Texture
- register
- custom/shader.rs
- InventoryProbe
- PostProcess
- Port<P>
- atomic
- script_startup/creature.rs
- world
- world/generation/tests.rs
- Declarations
- entity_checkpoint/tests.rs
- render/effects.rs
- plan
- UiFrame<'_>
- drops/queries.rs
- decode_transaction
- effects/registered.rs
- gameplay/decisions.rs
- Adapter
- handles.rs
- bloxgloom_host_api
- systems/world.rs
- KilnPayload
- custom.rs
- init
- Scripting capabilities for mod developers
- content/composition.rs
- Chunk
- ClientApp
- EntityTransferPolicy
- UiRenderer
- AnchoredBlockEntity
- Item
- EntityDefinition
- snapshots.rs
- script_startup/gameplay.rs
- client/bundle.rs
- sealed_neighborhood
- Clock
- parallel/tests.rs
- Plan: built-in/mod capability parity
- sync
- material.rs
- Adapter
- script/gameplay.rs
- Runtime
- declarer
- Adapter
- Complete modding implementation proposal
- Update
- client/startup.rs
- VisualFire
- render/tests.rs
- machine_component_tests.rs
- EffectKindId
- view
- extension_lifecycle.rs
- SystemDescriptor
- neighborhood.rs
- Proposal: one coherent gameplay API
- CoordinatorContext
- owner_durable/tests.rs
- StorageBlockEntity
- Extension
- wake.rs
- VisualAvatar
- Bindings
- ContentManifest
- Preparation
- prepare
- .bind_machine
- Ignitions
- position_store.rs
- Value
- MODDING-SURFACE-PLAN.md
- Context<'_>
- fs
- bounded.rs
- script_startup/appearance.rs
- pipeline.rs
- .handle
- entity_sleep.rs
- EntityDependencies
- Adapter
- MachinePayload
- inventory
- script_startup/bundle.rs
- FirePending
- script/generation.rs
- Startup
- authored.rs
- src/composition.rs
- Patrol
- io
- client/lifecycle/tests.rs
- entities
- model.rs
- package/tests.rs
- seed
- declarer
- slot.rs
- invoke
- items.rs
- DroppedItem
- Resources
- Gpu
- streaming.rs
- GpuPass
- Hit
- system/intents.rs
- journal/recovery.rs
- client/world.rs
- registry/tests.rs
- BootstrapContract
- Imports
- OwnerApplyReceipt
- Registry
- transfer.rs
- package/manifest.rs
- .register_action
- server/appearance.rs
- anchored_tests.rs
- .finish
- burn.rs
- ScriptMachine
- .public_view
- Invalid
- files.rs
- Wander
- EffectConsumerOutput
- .new
- notifications.rs
- server/checkpoint/tests.rs
- visibility.rs
- bundle_ui.rs
- server/gameplay/entities.rs
- ItemIcon
- WorkstationView
- .window_event
- tick/tests.rs
- entities/container.rs
- script_startup/machine.rs
- Behavior
- .bind_mobile
- Inputs
- .draw_node
- admin/tests.rs
- server/perf.rs
- Downloads
- receive_content_manifest
- SignalPost
- Input
- inventory/container.rs
- KilnFacing
- combined.rs
- run_loop
- server/drops.rs
- client_metadata.rs
- BundleIdentity
- raycast_blocks
- CodecProbe
- .register_inventory_screen
- route_registered_effects
- script_startup/gameplay/entities.rs
- script_startup/gameplay/inventory.rs
- Harvest
- position_store/tests.rs
- chest.py
- complete_barrier
- FireAnimator
- Catalog
- queries/tests.rs
- tests/anchored.rs
- Replace
- key
- .resolve
- prepare
- script_startup/generation.rs
- install
- declarer
- streaming/entities.rs
- publication.rs
- src/actions/tests.rs
- Shared<T>
- lifecycle-fixture/src/machine.rs
- Behavior
- Bloxgloom interface plan
- entity
- DropPolicy
- Remaining work by phase (living checklist)
- navigation.rs
- Resolved
- commands.rs
- script_startup/player.rs
- owner_wave/tests.rs
- mlua
- public_systems/tests.rs
- scheduling_tests.rs
- Bloxgloom
- chunk_loader/tests.rs
- drop_merge.rs
- Shared
- coordinator/tests.rs
- startup/block.rs
- journal.rs
- refund_stacks
- validate_spawn
- SharedParts
- decode
- EffectConsumerScratch
- declarer
- .first_solid_top
- lifecycle-fixture/src/content.rs
- journal/tests.rs
- generation
- coder.md
- .validate_player_selection
- coder-fast.md
- fields
- .handle
- .handle
- .deposit
- visual_contracts.rs
- decode
- spawn.rs
- Context
- Atlas
- WorldTime
- CheckpointWork
- LifecyclePlan
- .plan
- render.rs
- config/tests.rs
- resolve_player_movement
- avatars/tests.rs
- register
- MobilePages
- server/entities/kiln.rs
- decode
- .run
- validate_changes
- script_startup/drop_policy.rs
- parallel.rs
- .rotate_using
- outbound/tests.rs
- plan_changes
- render/effects/tests.rs
- .new
- transfer
- Public dynamic-entity surface
- script/tests.rs
- .owner_systems
- Public storage lifecycle boundary
- handler_declarer
- Registered content and composition
- Cross-cutting integration findings
- drop_pickup.rs
- declarer
- .append_batch
- Capture
- Registered actions and composed controls
- client/appearance.rs
- plan_motion
- predict_player_movement
- visual/tests.rs
- falling/tests.rs
- run
- client/drops/tests.rs
- custom/tests.rs
- storage/policy.rs
- join_lifecycle.rs
- receipts/tests.rs
- .compose_current_package_action_with_args
- quad
- write_frame
- .prepare_benchmark_frontier_wave
- .generate
- publication/tests.rs
- system/decisions.rs
- coder-smart.md
- Phase 8: examples, parity and integrated verification
- transfer/tests.rs
- effects/shader.rs
- Agent guidance
- Authored materials and effects
- replant.rs
- passes
- .register
- avatars/appearance.rs
- Definitions, assets, and composition
- TransferSelection
- gameplay
- bundle_runtime.rs
- handles/tests.rs
- inbox
- Modding: start here
- src/lifecycle/tests.rs
- reviewer.md
- composed.rs
- std
- open
- .install_egui
- bloxgloom
- blocks/SOURCES.md
- foliage/SOURCES.md
- EGUI-POC.md
- verdant/assets/fonts/FONT.md
- fonts/README.md
- ui-entity-actions/packages/uitarget/assets/fonts/FONT.md
- ui-target-actions/packages/uitarget/assets/fonts/FONT.md
- tests/writer.rs
- target_actions.rs
- palette/tests.rs
- .encode_source_patches
- time.rs
- combined/mixed.rs
- TEMP_ID
- decode
- prepare
- validate
- frozen_wake_registry

## God Nodes (most connected - your core abstractions)
1. `EntityError` - 233 edges
2. `OwnerKey` - 138 edges
3. `SystemId` - 133 edges
4. `StateKey` - 116 edges
5. `TickId` - 101 edges
6. `ClientMessage` - 88 edges
7. `world_to_chunk()` - 85 edges
8. `CommitAction` - 80 edges
9. `temp_save_dir()` - 80 edges
10. `VoxelView` - 77 edges

## Surprising Connections (you probably didn't know these)
- `F4 — New flammability meets a separate edit producer` --references--> `apply_synced_batch()`  [EXTRACTED]
  docs/modding/history/MODDING-PARITY-AUDIT.md → src/server/durable/fire.rs
- `private_entity_overlay_does_not_authorize_the_next_decision_owner()` --calls--> `Probe`  [INFERRED]
  crates/host-api/src/gameplay/tests.rs → extensions/lifecycle-fixture/src/system.rs
- `Entities, player behavior, world simulation and generation` --references--> `parse()`  [EXTRACTED]
  docs/modding/history/MODDING-PARITY-AUDIT.md → src/client/admin.rs
- `Entities, player behavior, world simulation and generation` --references--> `DropAnimator`  [EXTRACTED]
  docs/modding/history/MODDING-PARITY-AUDIT.md → src/client/drops.rs
- `Definitions, assets, and composition` --references--> `placeable_block_in()`  [EXTRACTED]
  docs/modding/history/MODDING-PARITY-AUDIT.md → src/items.rs

## Import Cycles
- 2-file cycle: `src/server/perf/tcp.rs -> src/server/perf/tcp/report.rs -> src/server/perf/tcp.rs`
- 2-file cycle: `src/server/entities/spatial.rs -> src/server/entities/store.rs -> src/server/entities/spatial.rs`
- 2-file cycle: `src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server/block_actions.rs`
- 3-file cycle: `src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server/net.rs`
- 3-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server.rs`
- 3-file cycle: `src/server.rs -> src/server/block_actions.rs -> src/server/durable/actions/mod.rs -> src/server.rs`
- 3-file cycle: `src/server/parallel.rs -> src/server/parallel/owner_wave.rs -> src/server/registry.rs -> src/server/parallel.rs`
- 4-file cycle: `src/server.rs -> src/server/net.rs -> src/server/net/reactor.rs -> src/server/net/reactor/connection.rs -> src/server.rs`

## Communities (516 total, 139 thin omitted)

### Community 0 - "entities/persistence.rs"
Cohesion: 0.07
Nodes (49): checked_body(), crc32(), Decoder, ENTITY_ALLOCATOR_MAGIC, ENTITY_ALLOCATOR_VERSION, ENTITY_CELL_VALUE_MAGIC, ENTITY_CELL_VALUE_VERSION, ENTITY_CHUNK_PAGE_MAGIC (+41 more)

### Community 1 - "EntityStore"
Cohesion: 0.06
Nodes (36): validate_location_owner(), validate_ownership_mode(), allocator_state_key(), apply_operation_to_projection(), canonical_location(), cell_state_key(), chunk_state_key(), collect_operations() (+28 more)

### Community 2 - "entities/types.rs"
Cohesion: 0.06
Nodes (16): AnchorUpdate, CellCoord, EntityId, EntityOwner, EntityOwnership, EntityView, MAX_ENTITY_FOOTPRINT_CELLS, MAX_ENTITY_PAYLOAD_BYTES (+8 more)

### Community 3 - "super"
Cohesion: 0.02
Nodes (7): abandoned_result_retires_transport(), completed_result_can_still_be_cancelled(), failure_and_retry_dispatch(), CYCLE_MS, INITIAL_MS, Codec, scenarios_really_cover_clustered_and_spread_player_layouts()

### Community 4 - "preview.rs"
Cohesion: 0.05
Nodes (50): render_block_preview(), CLIENT_MESH_RESULT_BATCH, CLIENT_PENDING_UPLOADS, render_daylight_previews(), DropPhase, read_rgba_png(), render(), render_egui_previews() (+42 more)

### Community 5 - "state_for"
Cohesion: 0.10
Nodes (40): drain_durable(), drop_active_len(), drop_candidates(), drop_nearby(), drop_stack(), reside_neighbourhood(), spawn_drop(), stage_entity_batch() (+32 more)

### Community 6 - "EntityTypeDescriptor"
Cohesion: 0.05
Nodes (16): AnchorEchoTick, BadTick, CounterInteract, CounterTick, FarReadTick, PlayerEchoTick, PokeBlind, PokeConsumer (+8 more)

### Community 7 - "world_to_chunk"
Cohesion: 0.12
Nodes (45): public_spawn_and_self_removal_are_one_atomic_recoverable_transaction(), bin_catalog(), bin_count(), bin_startup(), BinPayload, BinPullTick, coordinator_drain_preserves_deferred_entity_tick_until_commit(), drive_poke_tick() (+37 more)

### Community 8 - "CommitAction"
Cohesion: 0.05
Nodes (29): BlockActionContext, BlockActionHooks, BlockActionRegistry, BlockActionRegistryBuilder, BlockActionRegistryBuilder<'a>, BlockCommitBuilder, invoke_hook(), MAX_BLOCK_ACTION_HANDLERS (+21 more)

### Community 9 - "OwnerData"
Cohesion: 0.07
Nodes (9): OwnerCodec, OwnerData, OwnerCodecError, OwnerValueCodec, BytesCodec, U64Codec, saturating_wake_emitter(), StartupOwnerU64Codec (+1 more)

### Community 10 - "server/runtime/tests.rs"
Cohesion: 0.07
Nodes (38): captured_owner_reads_share_reservations_and_fence_exclusive_waves(), rejected_and_unconfirmed_owner_waves_publish_no_wakes_or_cursor(), mixed_active_and_recurring_due_owners_progress_with_and_without_wakes(), wake_advances_deadline_work_and_handler_can_return_to_active(), arbitrated_retry_withdraws_staged_wake_flags(), chunk_owner(), deferred_producer_waves_restage_their_durable_wakes(), dishonest_effect_accounting_is_rejected() (+30 more)

### Community 11 - "src/content.rs"
Cohesion: 0.05
Nodes (36): ACTIVE, block_flags(), BUILTIN_EMISSION, BUILTIN_FLAGS, BUILTIN_REFLECTANCE, CHEST_BLOCK_TYPE, CHEST_ENTITY_TYPE, CHEST_ITEM (+28 more)

### Community 12 - "VoxelView"
Cohesion: 0.07
Nodes (12): CapturedColumn, DropTickPlanner, BinExchange, Planner, Adapter, EntityBlockStateChange, EntityInteractionPlan, EntityTickPlan (+4 more)

### Community 13 - "OwnerKey"
Cohesion: 0.05
Nodes (18): FireDeliveryHandler, ChunkKey, MAX_EFFECTS_PER_OWNER_JOB, MAX_OWNER_PATCH_BYTES_PER_JOB, MAX_OWNER_PATCH_WRITES_PER_JOB, MAX_OWNER_WAVE_PATCH_BYTES, MAX_OWNER_WAVE_PATCH_WRITES, OwnerJob (+10 more)

### Community 14 - "scheduler.rs"
Cohesion: 0.08
Nodes (19): cursor_lane(), FIRE_DELIVERY_SYSTEM_ID, FIRE_LANES, FIRE_SYSTEM_ID, FireCursor, FireLoadMetrics, FireRecovered, FireRuntime (+11 more)

### Community 15 - "server/durable.rs"
Cohesion: 0.05
Nodes (17): Durability, BlockDelta, CHECKPOINT_QUEUE_CAPACITY, CHECKPOINT_WORKERS, DirtyCheckpoint, Durability, FireCheckpointBatch, MAX_DEFERRED_DURABLE_ACTIONS (+9 more)

### Community 16 - "IntentOutbox"
Cohesion: 0.16
Nodes (4): EntityChange, IntentOutbox, IntentRequest, Plan

### Community 17 - "OwnerEffectPatch"
Cohesion: 0.10
Nodes (5): blocked(), EmittedOwnerEffect, OwnerEffectPatch, route_and_consume(), RoutedOwnerWakes

### Community 18 - "SystemId"
Cohesion: 0.05
Nodes (42): OwnerPartition, SystemId, crc32(), decode_cell_value(), decode_cursor_value(), decode_owner_cursor_key(), decode_owner_state_key(), encode_cell_value() (+34 more)

### Community 19 - "src/storage.rs"
Cohesion: 0.08
Nodes (29): checksum(), CONTENT_MAP, CONVERSION_INCOMPLETE, FORMAT_VERSION, HEADER_LEN, invalid_data(), MAGIC, MAX_SNAPSHOT_BYTES (+21 more)

### Community 20 - "PackageSnapshot"
Cohesion: 0.06
Nodes (13): authored_drop_animation_negotiates_and_default_keeps_old_bundle(), drop_size_option_negotiates_verified_catalog_and_explicit_normal_preserves_bundle(), item_sprite_option_is_negotiated_and_omission_preserves_default_identity(), error(), MAX_ASSET_BYTES, MAX_ASSETS, MAX_MANIFEST_BYTES, MAX_MODULES (+5 more)

### Community 21 - "Cell"
Cohesion: 0.09
Nodes (8): Cell, Block, Context, Context<'a>, DropSpawn, Error, Plan, Snapshot

### Community 22 - "protocol.rs"
Cohesion: 0.07
Nodes (51): BLOCK_COUNT, Cursor, Cursor<'a>, frame(), invalid(), key(), MAX_ENTITY_INTERACT_BYTES, MAX_FIRE_BURSTS (+43 more)

### Community 23 - "Network"
Cohesion: 0.10
Nodes (12): connect_bundle_probe(), connect_catalog_probe(), connect_inventory_probe(), connect_ui_probe(), connect_visual_probe(), Incoming, Mesher, MesherJob (+4 more)

### Community 24 - "OutboundFrame"
Cohesion: 0.10
Nodes (11): ClientQueueTelemetry, OUTBOUND_AGGREGATE_BYTE_CAPACITY, OUTBOUND_CLIENT_BYTE_CAPACITY, OUTBOUND_FRAME_CAPACITY, OutboundError, OutboundFrame, OutboundQueue, OutboundTelemetry (+3 more)

### Community 25 - "StateKey"
Cohesion: 0.10
Nodes (24): _journal_domains_are_sorted(), decode_snapshot(), encode_snapshot(), FILE_NAME, MAGIC, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_KEYS, read_snapshot_file() (+16 more)

### Community 26 - "WorldSnapshot"
Cohesion: 0.09
Nodes (10): block(), combine_entities(), dispatch_neighbors(), error(), OperationInput, Participants, plan_removals(), plan_with_lifecycles() (+2 more)

### Community 27 - "MobileProbe"
Cohesion: 0.06
Nodes (10): MobileProbe, NetworkedVisualProbe, console_commands_reject_non_admin_and_recover_grant_over_nonblocking_listener(), creature_probe(), external_creature_spawns_moves_targets_interacts_and_recovers_over_real_listener(), mixed_response_path_keeps_edits_creatures_and_machine_progressing_across_restart(), mixed_work(), response_samples() (+2 more)

### Community 28 - "receipts.rs"
Cohesion: 0.10
Nodes (18): Admission, checksum(), invalid(), MAGIC, MAX_PAYLOAD, MAX_REASON, MAX_SNAPSHOT, ReceiptEvent (+10 more)

### Community 29 - "kiln/planning.rs"
Cohesion: 0.17
Nodes (17): block_state_changes(), bump_inventory_revision(), burn_one_step(), insert_output(), KilnBreakPlan, KilnInsertPlan, KilnInteractionPolicy, KilnTakePlan (+9 more)

### Community 30 - "handler.rs"
Cohesion: 0.08
Nodes (14): FireFrontier, FireDeliveryInput, FireDeliveryPatch, FireHandler, FireOwnerInput, FireOwnerPatch, MAX_DELIVERIES_PER_OWNER, MAX_DUE_CELLS_PER_OWNER (+6 more)

### Community 31 - "Handler"
Cohesion: 0.11
Nodes (18): Handler, Destroy, GroundRemoved, Neighbor, Removed, ChestBreak, CollectDrop, FlowerNeighbor (+10 more)

### Community 32 - "ui/draw.rs"
Cohesion: 0.10
Nodes (23): UiBuilder<'_>, UiBuilder<'_>, EDGE, FONT_HEIGHT, FONT_WIDTH, GOLD, inset(), item_color() (+15 more)

### Community 33 - "PendingWakeStore"
Cohesion: 0.08
Nodes (24): crc32(), decode_owner_wake_key(), decode_wake_value(), encode_wake_value(), invalid_data(), OWNER_WAKE_DOMAIN, owner_wake_key(), OWNER_WAKE_MAGIC (+16 more)

### Community 34 - "server_state_with_startup"
Cohesion: 0.12
Nodes (39): tick_once(), server_state_with_startup(), durable_counter_startup(), durable_pair_startup(), durable_twin_startup(), entity_and_owner_state_commit_as_one_atomic_record(), external_neighbor_reads_defer_until_all_chunks_arrive_and_fence_adjacent_edits(), external_owner_can_durably_wake_another_owner_after_restart() (+31 more)

### Community 35 - "collections"
Cohesion: 0.08
Nodes (17): _entity_cell_key_round_trip(), _entity_page_key_round_trip(), write_checkpoint(), Bucket, ChunkPage, decode_cell_key(), decode_cell_owner(), decode_chunk_key() (+9 more)

### Community 36 - "EntityCodecError"
Cohesion: 0.07
Nodes (14): BinCodec, CounterCodec, MateCodec, WideCodec, ByteCodec, Codec, decode_stack(), encode_payload() (+6 more)

### Community 37 - "Inventories"
Cohesion: 0.10
Nodes (6): handler_random_is_stable_per_seed_cell_and_registration(), ignored_failures_cannot_publish_partial_operations(), Inventories, private_entity_overlay_does_not_authorize_the_next_decision_owner(), transfers_preserve_components_and_failed_capacity_checks_preserve_both_sides(), World

### Community 38 - "IntentDelivery"
Cohesion: 0.09
Nodes (20): IntentDelivery, blocked(), decode(), decode_key(), encode(), is_key(), key(), Reader (+12 more)

### Community 39 - "SystemRuntime"
Cohesion: 0.08
Nodes (9): SystemRuntime, MAX_OWNER_VALUES_PER_SYSTEM, MAX_PENDING_OWNER_WAKES, PendingRegisteredWave, PreparedRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs, StagedOwnerCommit (+1 more)

### Community 40 - "journal/rotation.rs"
Cohesion: 0.15
Nodes (37): crc32(), invalid_data(), Base, BASE_FORMAT_VERSION, BASE_FORMAT_VERSION_LEGACY, BASE_MAGIC, BASE_MAX_BYTES, base_path() (+29 more)

### Community 41 - "join"
Cohesion: 0.09
Nodes (33): profile_appearance_corruption_fails_closed_and_missing_profile_keeps_default(), profile_appearance_save_failure_never_publishes_and_stops_mutation(), failed_startup_queue_does_not_register_a_ghost_profile(), full_outbound_queue_disconnects_only_the_slow_client(), joined_players_spawn_above_solid_terrain_with_headroom(), joins_find_lower_safe_surface_after_origin_support_is_mined(), multiple_clients_receive_edit_delta_then_resync_snapshot_in_order(), remote_player_spawn_move_and_leave_publish_ordered_entity_changes() (+25 more)

### Community 42 - "server.rs"
Cohesion: 0.05
Nodes (35): catalog_with_extension(), Client, DEFAULT_CLIENTS, DEFAULT_VIEW, EDIT_REACH, handle_message(), INPUT_CAPACITY, join_client() (+27 more)

### Community 43 - "request_chunk"
Cohesion: 0.08
Nodes (19): block_intersects_player(), denied(), Invocation, plan(), read_target(), sight(), verify_reach(), plan() (+11 more)

### Community 44 - "drops/planning.rs"
Cohesion: 0.12
Nodes (25): merge_target(), plan_error(), plan_expired(), plan_spawn_stack(), plan_spawns(), plan_spawns_with_extra(), plan_stack_spawns(), plan_stack_spawns_with_extra() (+17 more)

### Community 45 - "MobileEntity"
Cohesion: 0.09
Nodes (16): Animation, Behavior, Body, Context, Cuboid, Error, Lifecycle, MobileEntity (+8 more)

### Community 46 - "Renderer"
Cohesion: 0.06
Nodes (5): next_upload_index(), order_pending_mesh(), Renderer, urgent_mesh_reorders_existing_pending_chunk_without_duplication(), create_depth()

### Community 47 - "UiLayout"
Cohesion: 0.10
Nodes (11): InventorySearch, search_rect(), join_action_rect(), centered_panel(), effective_ui_scale(), HitRect, UiLayout, SettingId (+3 more)

### Community 48 - "server/fire/tests.rs"
Cohesion: 0.12
Nodes (32): cursor_key(), frontier_key(), benchmark_frontier_bootstrap_precedes_first_live_fire_tick(), checkpoint_batch_replaces_one_complete_snapshot_and_applies_tombstones(), checkpoint_store_rejects_orphans_and_corruption_and_cleans_interrupted_temp(), chunk(), durable_lane_age_prioritizes_an_owner_deferred_by_wal_pressure(), first_aggregate_write_preserves_legacy_per_key_checkpoints() (+24 more)

### Community 49 - "EntityError"
Cohesion: 0.08
Nodes (9): Decoder<'a>, Encoder, Body, DT, Movement, valid_position(), PreparedEntityTransaction, EntityView (+1 more)

### Community 50 - "server/effects.rs"
Cohesion: 0.10
Nodes (25): block_change_owners(), boundary_coordinates(), CellCoord, Effect, EffectBatch, EffectBuffer, EffectBufferError, EffectEnvelope (+17 more)

### Community 51 - "EntityPublicView"
Cohesion: 0.08
Nodes (11): MAX_PLAYER_ENTITY_PAYLOAD_BYTES, MAX_SESSION_PLAYER_ENTITIES, PLAYER_ENTITY_TYPE, player_public_payload_codec_is_fixed_size(), player_type_registers_only_against_catalogued_identity(), PlayerEntityPayload, PlayerEntityStore, PlayerPayloadCodec (+3 more)

### Community 52 - "bench.rs"
Cohesion: 0.08
Nodes (17): FireApplyTimings, FireRuntime, ACTIVE_CHUNKS, benchmark_cpu(), BenchSave, cell_index(), CHUNKS_X, CHUNKS_Z (+9 more)

### Community 53 - "intent/tests.rs"
Cohesion: 0.11
Nodes (32): durable_intent_bootstrap_capacity_and_cancellation_do_not_leave_orphans(), durable_intent_bootstrap_combines_producers_and_ordinals_without_duplicate_creation(), durable_intent_bootstrap_destination_conflict_retries_one_atomic_record(), durable_intent_bootstrap_existing_destination_wins_and_opt_in_is_required(), durable_intent_bootstrap_prepared_waves_reserve_capacity_across_systems(), cell(), chunk(), durable_intent_bootstrap_chain_gates_creation_retry_forwarding_and_restart() (+24 more)

### Community 54 - "ScriptError"
Cohesion: 0.12
Nodes (11): CacheKey, decode(), encode(), float(), command_metadata_roundtrips_schema_and_permissions_and_rejects_forgery(), invalid(), ordered(), Reader (+3 more)

### Community 55 - "Replicas"
Cohesion: 0.08
Nodes (15): Interaction, presentation and durability closure, Assembly, kiln_adapter(), MAX_CHUNK_ENTITY_BYTES, MAX_CLIENT_ENTITY_BYTES, MAX_PENDING_BYTES, MAX_PENDING_COMMITS, MAX_PENDING_SNAPSHOTS (+7 more)

### Community 56 - "src/client/tests.rs"
Cohesion: 0.07
Nodes (19): action_ids_keep_session_and_order_without_reuse(), block_edit_uses_selected_hotbar_block_and_hit_face(), confirmed_fire_visuals_expire_and_are_capped_and_distance_culled(), graphics_controls_apply_save_and_preserve_values_while_disabled(), lamp_edit_rebuilds_both_sides_of_a_chunk_seam_urgently(), latest_edit_mesh_survives_a_superseded_kiln_relight_backlog(), mapped_server_item_and_replaceable_state_drive_placement_preview(), moving_object_lighting_keeps_completed_field_during_relight_then_accepts_darkness() (+11 more)

### Community 57 - "ServerStartup"
Cohesion: 0.11
Nodes (5): entity_error(), ServerStartup, ServerStartup, StartupEntityType, StartupOwnerCodec

### Community 58 - "script.rs"
Cohesion: 0.10
Nodes (13): Invocation, LimitExceeded, Limits, Output, Program, Request, run(), run_presentation() (+5 more)

### Community 59 - "Registrar"
Cohesion: 0.12
Nodes (3): CubeBlock, Registrar, RegistrationError

### Community 60 - "client/entities/tests.rs"
Cohesion: 0.10
Nodes (29): anchored_presentation_window_is_separate_scoped_and_sorted(), mossbun_adapter_uses_the_negotiated_catalog_assignment(), mossbun_adapter_validates_payload_and_tracks_snapshot_removal_and_eviction(), presentation_entity_window_is_ordered_and_explicitly_bounded(), accept(), block_and_entity_changes_wait_for_whole_cross_chunk_commit(), checksum_conflict_and_revision_gap_request_resync_without_partial_install(), chunk() (+21 more)

### Community 61 - "server/drops/tests.rs"
Cohesion: 0.11
Nodes (30): active_len(), apply_expired(), assert_store_consistent(), drop_world(), drop_world_in(), DropWorld, insert_entry(), item() (+22 more)

### Community 62 - "PhaseExecutor"
Cohesion: 0.14
Nodes (17): BarrierError, CancellationToken, execute_task(), JobCompletion, JobOutcome, OwnerResults, PendingBatch, PhaseExecutor (+9 more)

### Community 63 - "perf/fixture.rs"
Cohesion: 0.09
Nodes (23): ACTION_INTERVAL, add_clients_and_seed_drops(), DIRT_ITEM, drain_outbound(), DrainTotals, DROP_HEIGHTS, install_chunks(), MAX_STEADY_TICKS (+15 more)

### Community 64 - "World"
Cohesion: 0.09
Nodes (6): ChunkKey, ChunkReadStamp, EditBasis, LoadedChunk, PreparedEdit, World

### Community 65 - "stack"
Cohesion: 0.16
Nodes (8): advance(), chest_hopper_chest_chain_preserves_last_slot_components_restart_and_refunds(), contents(), resident(), transfer(), hopper(), pulse(), registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes()

### Community 67 - "host-api/src/machine.rs"
Cohesion: 0.22
Nodes (10): Context, FACES, Filter, Fuel, Machine, Port, Process, Recipe (+2 more)

### Community 68 - "PalettedBlocks"
Cohesion: 0.10
Nodes (6): LocalIndex, PalettedBlocks, PaletteView, set_palette_cell(), u16, u8

### Community 69 - "server/entities/tests.rs"
Cohesion: 0.15
Nodes (23): decode_checkpoint(), encode_checkpoint(), a_frozen_type_registry_requires_every_catalogued_type_and_valid_anchor_schema(), anchored_footprint_indexes_both_sides_of_negative_chunk_seam_atomically(), checkpoint_round_trip_rebuilds_indexes_and_rejects_corruption_or_unknown_types(), delayed_payload_receipt_merges_with_newer_checkpointed_mobile_motion(), DROP_TYPE, fake_neighbour() (+15 more)

### Community 70 - "conflict_tests.rs"
Cohesion: 0.12
Nodes (25): action(), coordinator_admits_two_independent_atomic_pickups_before_applying_either(), disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply(), drop_merge_absence_is_fenced_against_same_owner_motion_into_range(), hold(), neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders(), overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership(), plan() (+17 more)

### Community 71 - "GenerationError"
Cohesion: 0.10
Nodes (11): CHUNK_SIZE, Context, Contributor, GenerationError, in_world_bounds(), MAX_WRITES, mix(), Output (+3 more)

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
Cohesion: 0.07
Nodes (4): action_id(), PackageActionProbe, PackageActionProbe, ClientMessage

### Community 76 - "PublicEntity"
Cohesion: 0.10
Nodes (29): Replicas, BlockCellChange, enforce_frame_size(), EntitySnapshotPage, MAX_BLOCK_CHANGES_PER_PART, MAX_ENTITIES_PER_PAGE, MAX_ENTITY_CHANGES_PER_PART, MAX_ENTITY_SNAPSHOT_BYTES (+21 more)

### Community 77 - "invoke"
Cohesion: 0.13
Nodes (11): cell_at(), checked(), entity_id(), invalid(), invoke(), position_at(), install(), latch() (+3 more)

### Community 78 - "key"
Cohesion: 0.10
Nodes (26): animated_item_bundle_rejects_invalid_and_noncanonical_motion(), canonical_order_dependency_identity_and_count_bounds_are_verified(), decoder_rejects_server_classification_and_oversized_payloads_before_copying(), bundle(), DESCRIPTOR, SHADER, verified_bundle_prepares_effect_and_rejects_ownership_order_and_shader_failures(), header() (+18 more)

### Community 79 - "Receiver"
Cohesion: 0.10
Nodes (20): Receiver, ACCEPT_BUDGET, has_admission_capacity(), has_admission_capacity_with_limit(), inventory_worker(), INVENTORY_WORKERS, InventoryLoadRequest, InventoryWorkers (+12 more)

### Community 80 - "config.rs"
Cohesion: 0.08
Nodes (18): ConfigWriter, clamp_finite(), Config, CONFIG_VERSION, create_temporary_file(), MAX_FOV, MAX_SCALE, MAX_SENSITIVITY (+10 more)

### Community 82 - "src/world.rs"
Cohesion: 0.09
Nodes (31): AIR, BEDROCK_Y, BLUE_FLOWER, CHUNK_SIZE, CHUNK_VOLUME, DIRT, FERN, GLOWSTONE (+23 more)

### Community 83 - "Inventory"
Cohesion: 0.06
Nodes (22): ComponentPayload, HOTBAR_SLOTS, Inventory, MAX_COMPONENT_BYTES, SLOTS, Stack, STACK_LIMIT, checksum() (+14 more)

### Community 84 - "Change"
Cohesion: 0.07
Nodes (19): Durability, CommitBarrier, Change, arbitrate_key_sets(), build_owner_writes_parallel(), canonical_key_set(), OwnerCommit, OwnerWaveDurables (+11 more)

### Community 85 - "TickSample"
Cohesion: 0.11
Nodes (13): duration_nanos(), EVENT_LATENCY_STREAMS, LatencyEvent, LatencyRing, Metric, MetricsRecorder, nearest_rank(), PHASE_COUNT (+5 more)

### Community 86 - "tcp.rs"
Cohesion: 0.08
Nodes (20): TransportSnapshot, OutboundSnapshot, drive(), exercise(), movement(), open_nuisance_peers(), PROFILE_BASE, percentile() (+12 more)

### Community 87 - "Pending"
Cohesion: 0.08
Nodes (8): Declarations, invoke(), MAX_BLOCKS_PER_PACKAGE, MAX_ITEMS_PER_PACKAGE, MAX_TEXTURES_PER_PACKAGE, PackageTexture, Pending, text()

### Community 88 - "GameUi"
Cohesion: 0.08
Nodes (5): draw_screen(), DrawTarget, GameUi, Intent, themed_context()

### Community 89 - "actions/entity.rs"
Cohesion: 0.17
Nodes (19): capture_dependencies(), capture_entity_view_for_plan(), capture_tick_input(), capture_view_for_plan(), commit_tick_plan(), corrupt(), interaction_sight(), permission() (+11 more)

### Community 90 - "owner/tests.rs"
Cohesion: 0.11
Nodes (19): Behavior, CELL, drop_count(), empty_action(), Fixture, Harvest, KEY, Marker (+11 more)

### Community 91 - "Effect"
Cohesion: 0.12
Nodes (3): Effect, target_sizes(), targets_remain_bounded_at_large_and_tiny_viewports()

### Community 92 - "JournalWriter"
Cohesion: 0.09
Nodes (7): CommitReceipt, Journal, JournalWriter, Request, RotateError, RotationReceipt, WriterCommand

### Community 93 - "modding/README.md"
Cohesion: 0.10
Nodes (18): Chunk generation for native contributors, Luau authoring in VS Code, Validate, Authored UI widgets and current limits, Exact identities and time, Local Luau packages, Package shape, Try the UI example (+10 more)

### Community 94 - "CheckpointWriter"
Cohesion: 0.10
Nodes (7): checkpoint_shard(), checkpoint_worker(), CheckpointJob, CheckpointReceipt, CheckpointSubmitError, CheckpointWriter, panic_message()

### Community 95 - "src/client.rs"
Cohesion: 0.09
Nodes (15): ActionTracker, command_action_id(), digit_slot(), edit_for_hit(), edit_for_hit_with_catalog(), FRAME, INCOMING_FRAME_BUDGET, Keys (+7 more)

### Community 96 - "EntityCheckpointMirror"
Cohesion: 0.16
Nodes (9): CheckpointReceipt, CheckpointTicket, Command, EntityCheckpointMirror, Event, MAX_MIRROR_ADMISSIONS, MirrorMetrics, MirrorPermit (+1 more)

### Community 97 - "Connection"
Cohesion: 0.16
Nodes (6): Connection, PendingWrite, PendingWriteKind, Phase, PendingLeave, SimulationInput

### Community 98 - "ClientHandle"
Cohesion: 0.14
Nodes (9): ActionKind, active_slow_peer(), ClientHandle, ClientStats, handshake(), PendingAction, read_until_stop(), Ready (+1 more)

### Community 99 - "TickId"
Cohesion: 0.08
Nodes (26): apply_synced_batch(), Durability, run_delivery(), run_source(), stage_gameplay_burn(), stage_transactions(), stage_wave(), check_seeded_fire_restart() (+18 more)

### Community 100 - "Attempt"
Cohesion: 0.12
Nodes (5): Attempt, Control, Prepared, Progress, JoinProgress

### Community 101 - "durable/coordinator.rs"
Cohesion: 0.09
Nodes (30): batchable_motion(), cancel_prepared_entities(), command_action_id(), defer_action(), durable_request_profile(), fail_if_durability_failed(), fatal_stage_error(), finish_noncommand_request() (+22 more)

### Community 102 - "serve"
Cohesion: 0.10
Nodes (23): luau_action_block_targets_keep_real_reach_sight_and_identity_checks(), luau_creature_replaces_itself_with_another_authored_type_over_real_listener(), mod_admin_grant_requires_server_identity_and_replays_once_over_listener(), mod_admin_spawn_and_drop_share_one_allocator_and_restart(), connect(), package(), package_cube_flags_are_frozen_and_old_declaration_keeps_defaults(), package_cube_joins_places_and_recovers_with_identical_session_catalog() (+15 more)

### Community 103 - "package/client.rs"
Cohesion: 0.06
Nodes (34): ANIMATED_MAGIC, APPEARANCE_MAGIC, APPEARANCE_POLICY_MAGIC, BLOCK_OPTIONS_MAGIC, BLOCK_STATES_MAGIC, ClientBundle, ClientPackage, ClientSide (+26 more)

### Community 106 - "state.rs"
Cohesion: 0.11
Nodes (12): stage(), touches_anchor(), open(), action_changes(), chunk_state_key(), decode_chunk_key(), decode_profile_key(), encode_action_receipt() (+4 more)

### Community 107 - "Adapter"
Cohesion: 0.23
Nodes (3): Adapter, component_matches(), register()

### Community 108 - "dispatch.rs"
Cohesion: 0.14
Nodes (15): apply(), disconnect(), prepare(), Prepared, publish(), effect(), fire_cue_is_dropped_for_backlogged_client_without_disconnect(), fire_cue_requires_subscription_and_follows_committed_world_frame() (+7 more)

### Community 109 - "menus.rs"
Cohesion: 0.35
Nodes (11): actions(), admin(), button(), draw(), EDGE, GOLD, MUTED, PANEL (+3 more)

### Community 110 - "kiln/tests.rs"
Cohesion: 0.16
Nodes (9): kiln_footprint(), catalog_with_test_output(), codec_projects_workstation_contents_and_recipe_progress_without_components(), compiled_states_cover_both_halves_facings_and_lit_emission(), footprint_and_break_plans_cover_positive_and_negative_chunk_seams(), registry(), registry_rejects_duplicate_kiln_registration(), unsupported_input_is_valid_and_burning_fuel_expires_while_idle() (+1 more)

### Community 111 - "terrain.rs"
Cohesion: 0.17
Nodes (28): Biome, collapse_surface(), Column, decorate_chunk(), generate_blocks(), generated_block(), generated_block_in_column(), generated_block_with_pattern() (+20 more)

### Community 112 - "BatchId"
Cohesion: 0.11
Nodes (6): BatchId, CancelError, ExecutorConfigError, JobKey, PhaseExecutor<R, E>, SubmitError

### Community 113 - "ComponentMatch"
Cohesion: 0.15
Nodes (13): ComponentMatch, ComponentOutput, ComponentValue, value_bytes(), decode_exact(), decode_input(), decode_output(), encode_exact() (+5 more)

### Community 114 - "ChunkLoader"
Cohesion: 0.11
Nodes (9): ChunkLoader, ChunkLoadResult, ChunkLoadTicket, Job, RequestError, RequestStatus, stop_workers(), WORKER_COUNT (+1 more)

### Community 115 - "Journal"
Cohesion: 0.10
Nodes (5): clock_key(), frame_len(), Journal, KnownRecord, Transaction

### Community 116 - "script_startup/system.rs"
Cohesion: 0.12
Nodes (22): commit(), luau_system_multi_owner_wave_is_fresh_and_atomic_on_worker_failure(), Fixture, KEY, luau_burn_owner_uses_host_removal_semantics_and_persists_receipt(), luau_owner_after_dependencies_are_resolved_before_save_creation(), luau_owner_block_info_uses_captured_public_fields_and_restarts(), luau_owner_caught_invalid_entity_change_rejects_whole_wave() (+14 more)

### Community 118 - "world/tests.rs"
Cohesion: 0.10
Nodes (26): biomes_cover_distinct_surfaces_across_an_endless_world(), broadleaf_crowns_cross_chunk_seams_and_match_edit_baseline(), chunk_cache_evicts_the_least_recently_used_resident(), collapsed_surface_obeys_constraints_and_matches_region_edges(), composed_generation_is_ordered_and_does_not_change_builtin_baseline(), composed_generation_rejects_unknown_states_and_duplicate_keys(), corrupt_save_is_not_silently_discarded(), edited_chunk_can_be_evicted_and_reloaded_from_its_pending_snapshot() (+18 more)

### Community 119 - "temp_save_dir"
Cohesion: 0.16
Nodes (29): edit(), external_storage_lifecycle_seam_restart_retries_and_exact_refunds(), external_storage_rejects_blocked_footprint_and_stale_placement_without_debit(), ids(), open(), registered_slot_permissions_are_enforced_by_server_even_for_forged_requests(), resident(), startup() (+21 more)

### Community 120 - "PlayerRules"
Cohesion: 0.05
Nodes (41): Body, BUILTIN_BODY, BUILTIN_MOTION, BUILTIN_RULES, BUILTIN_SPAWN, InvalidPlayerRules, MotionRates, PlayerRules (+33 more)

### Community 121 - "entities/checkpoint.rs"
Cohesion: 0.13
Nodes (13): CHECKPOINT_NAME, checkpoint_rejects_bad_magic_version_and_checksum(), checkpoint_write_read_is_atomic_and_bounded(), crash_left_temporary_checkpoint_fails_closed(), DIRECTORY_NAME, entity_invalid_data(), EntityCheckpointStore, invalid_data() (+5 more)

### Community 122 - "plan_durable_request"
Cohesion: 0.13
Nodes (19): hopper_push_is_atomic_conflict_checked_and_resumes_after_full_destination(), plan_durable_request(), action_without_an_authoritative_handler_fails_before_world_creation(), probe_request(), register_probe(), registered_block_observation_is_required_and_remains_fenced_at_admission(), registered_inventory_uses_current_slots_and_checks_identity_reach_and_sight(), registered_recipe_rejects_full_output_components_stale_and_malformed_without_partial_debit() (+11 more)

### Community 123 - "Burn"
Cohesion: 0.16
Nodes (3): EditCause, Burn, BurnGrass

### Community 126 - "HarvestSnapshot"
Cohesion: 0.14
Nodes (5): cell_random(), flower_harvests_itself_and_grass_and_leaves_have_distinct_loot(), harvest(), harvest_with_catalog(), HarvestSnapshot

### Community 127 - "snapshots/tests.rs"
Cohesion: 0.15
Nodes (17): captured_revision_is_rejected_after_confirmed_edit_and_recaptured_in_order(), changed_interest_or_reconnected_session_cannot_receive_pending_capture(), dense_snapshot_disconnects_only_affected_client_and_closes_earlier_jobs(), differing_epochs_do_not_share_wire_content(), distinct_chunk_capture_is_bounded_and_rotates_to_deferred_clients(), Fixture, live_stream_prepares_once_and_shares_encoded_pages_for_matching_clients(), pressure_reclaim_is_worker_prepared_and_coordinator_announces_then_releases() (+9 more)

### Community 128 - "authored/tests.rs"
Cohesion: 0.14
Nodes (21): action_callback_cannot_supply_target_or_authorization_claims(), change_json(), decode(), dynamic(), egui_input_dispatches_unicode_text_and_preserves_busy_value(), encode(), encode_source(), handlers_fail_closed_atomically_with_module_attribution_and_sandbox_limits() (+13 more)

### Community 129 - "ScriptCreature"
Cohesion: 0.16
Nodes (3): invalid(), ScriptCreature, State

### Community 130 - "content"
Cohesion: 0.40
Nodes (3): MAX_BLOCKS, MAX_ITEMS, MAX_TEXTURES

### Community 131 - "net.rs"
Cohesion: 0.11
Nodes (15): BUNDLE_TIMEOUT, ContentHandshake, HELLO_TIMEOUT, JOIN_TIMEOUT, JoinCleanup, bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline(), commands_received_after_content_ready_wait_for_join_completion(), connected_streams() (+7 more)

### Community 132 - "ScriptSystem"
Cohesion: 0.12
Nodes (6): bytes(), cell(), declarer(), field(), ScriptSystem, table()

### Community 133 - "host-api/src/system.rs"
Cohesion: 0.15
Nodes (14): BlockEdit, DropSpawn, EntitySpawn, IntentId, MAX_INTENT_PAYLOAD_BYTES, MAX_INTENTS_PER_JOB, Owner, Partition (+6 more)

### Community 134 - "Payload"
Cohesion: 0.18
Nodes (6): Behavior, Context, interaction_request(), Reaction, RemovalCause, Payload

### Community 135 - "InventoryId"
Cohesion: 0.20
Nodes (7): Components, Context<'_>, InventoryId, PickupTransfer, PickupTransfer<'a>, Slot, Stack

### Community 136 - "VisualSession"
Cohesion: 0.14
Nodes (3): Script, PendingBatch, VisualSession

### Community 137 - "entity_recovery/tests.rs"
Cohesion: 0.16
Nodes (12): checkpointed_motion_ahead_of_wal_fence_survives_recovery(), fixture(), lagging_checkpoint_replays_later_wal_transfer(), MOBILE_TYPE, NEXT_DIR, one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt(), position(), same_revision_conflicting_checkpoint_motion_fails_closed() (+4 more)

### Community 138 - "RegisteredEffectError"
Cohesion: 0.21
Nodes (4): ErasedEffectKind, RegisteredEffectBuffer<'a>, RegisteredEffectError, TypedEffectKind<P, M, V, D, C>

### Community 139 - "host-api/src/actions.rs"
Cohesion: 0.13
Nodes (16): Action, CommandPermission, MAX_ACTIONS, MAX_INTERACTION_PAYLOAD, MAX_REQUEST_ARGUMENTS, MAX_TARGET_ACTIONS, MAX_WIDGETS, Operation (+8 more)

### Community 140 - "Appearance"
Cohesion: 0.08
Nodes (9): Appearance, MAX_ADDITIONS, MODEL, Catalog, PANTS, SHIRTS, SKINS, decode() (+1 more)

### Community 141 - "ObserverRegistration"
Cohesion: 0.10
Nodes (7): Committed, CommittedBlock, CommittedEntity, Observer, ObserverRegistration, Catalog, UseObserver

### Community 142 - "presentation.rs"
Cohesion: 0.20
Nodes (14): bounded_float(), Command, command_entity(), display_text(), EntityView, invalid(), optional_bounded_float(), Reply (+6 more)

### Community 143 - "render/mesh.rs"
Cohesion: 0.16
Nodes (10): ChunkMesh, emit_plant(), emit_quad(), GpuMesh, GpuSubmesh, mesh_chunk_lit(), mesh_chunk_lit_with_catalog(), mesh_chunk_with_catalog() (+2 more)

### Community 144 - "Buffer"
Cohesion: 0.08
Nodes (6): Buffer, capture(), default_filter_preserves_structured_game_events_and_flushes_final_errors(), scoped_filters_and_invalid_filter_fallback_work_without_global_state(), create_sky_pipeline(), create_target_pipeline()

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
Cohesion: 0.16
Nodes (11): apply(), BUILTIN_SAMPLES, builtin_state_key(), BuiltinSamples, compose(), generate_chunk(), generate_chunk_with_contributors(), generation_error() (+3 more)

### Community 150 - "publication/commit.rs"
Cohesion: 0.24
Nodes (10): add_remove(), add_upsert(), collect(), CommitChanges, CommitPlan, fanout_bound(), for_client(), KeyChanges (+2 more)

### Community 151 - ".activate_control"
Cohesion: 0.11
Nodes (8): BINDING_ROWS_PER_PAGE, binding_targets(), BindingTarget, ClientApp, Command, parse(), registered(), Builtin

### Community 152 - "StorageOwner"
Cohesion: 0.21
Nodes (3): StorageNeighbor, StorageOwner, StorageRemoved

### Community 153 - "HandlerRegistration"
Cohesion: 0.13
Nodes (4): Event, EventKind, HandlerRegistration, RemovalCause

### Community 154 - "InventoryScreen"
Cohesion: 0.18
Nodes (6): InventoryScreen, MAX_SLOTS, MAX_STATUS_FIELDS, SlotGroup, StatusField, StatusFormat

### Community 155 - "render/drops.rs"
Cohesion: 0.15
Nodes (17): block_and_sprite_drops_carry_sky_glow_and_bounce_without_fixed_lighting(), DropMeshes, emit_cutout_drop(), flower_pickup_uses_cutout_crosses_instead_of_cube_faces(), grass_side_band_is_at_the_top_on_both_side_axes(), is_sprite_item(), MAX_CUTOUT_INDEX_BYTES, MAX_CUTOUT_VERTEX_BYTES (+9 more)

### Community 156 - "Texture"
Cohesion: 0.19
Nodes (5): Texture, Catalog, error(), texture_definition(), validate_display()

### Community 157 - "register"
Cohesion: 0.13
Nodes (4): register(), Slots, StoragePayload, StoragePayload<N>

### Community 158 - "custom/shader.rs"
Cohesion: 0.09
Nodes (8): validate(), compose(), STUBS, TYPES, validate(), validate(), rename(), validate()

### Community 159 - "InventoryProbe"
Cohesion: 0.09
Nodes (4): InventoryProbe, external_item_action_composed_control_receipt_duplicate_stale_and_restart(), send(), send_with_session()

### Community 161 - "Port<P>"
Cohesion: 0.16
Nodes (4): block(), Interaction<P>, Port<P>, public_slots()

### Community 162 - "atomic"
Cohesion: 0.07
Nodes (11): CodecWorkers, decode_worker(), DECODE_WORKERS, DecodeRequest, encode_worker(), ENCODE_WORKERS, EncodedFrame, EncodeRequest (+3 more)

### Community 163 - "script_startup/creature.rs"
Cohesion: 0.14
Nodes (12): Flat, luau_creature_interaction_and_animation_negotiate_and_keep_private_state(), luau_creature_negotiates_model_ticks_and_restarts(), luau_creature_neighbour_policy_negotiates_and_reads_bounded_public_views(), luau_creature_options_reject_invalid_bounds_before_save_creation(), luau_creature_rejects_invalid_declaration_before_save_and_caught_route_failure(), luau_creature_rejects_invalid_lifecycle_before_movement_or_state_change(), luau_creature_spawns_another_declared_type_with_its_own_initial_state() (+4 more)

### Community 164 - "world"
Cohesion: 0.09
Nodes (12): air_chunk(), key(), MissingChunk, MovementError, player_collides(), resolve_player_movement(), set_block(), SnapshotError (+4 more)

### Community 165 - "world/generation/tests.rs"
Cohesion: 0.12
Nodes (11): absolute_anchor_feature_is_not_truncated_at_chunk_boundary(), AcrossSeam, authoritative_generation_edit_baselines_survive_cache_miss_and_restart(), builtin_contributor_preserves_terrain_vegetation_and_negative_chunk_seams(), builtin_contributor_preserves_tree_canopy_across_chunk_seam(), files(), generation_failure_never_installs_air_or_replaces_recovery_snapshot(), generation_identity_mismatch_rejects_before_any_save_mutation() (+3 more)

### Community 167 - "entity_checkpoint/tests.rs"
Cohesion: 0.18
Nodes (13): admitted_event_permit_cannot_be_dropped_silently(), checkpoint_io_failure_closes_admission_and_reports_error(), delayed_payload_receipt_keeps_newer_checkpoint_only_motion(), fixture(), interrupted_stream_does_not_publish_and_worker_failure_releases_credit(), malformed_ordered_event_fails_closed_without_checkpoint_publication(), multi_turn_checkpoint_holds_generation_fence_without_a_live_capture(), NEXT_TEST_DIR (+5 more)

### Community 168 - "render/effects.rs"
Cohesion: 0.18
Nodes (10): Descriptor, MAX_DESCRIPTOR_BYTES, MAX_PASSES, MAX_SHADER_BYTES, Pass, prepare(), prepare_inner(), Prepared (+2 more)

### Community 169 - "plan"
Cohesion: 0.24
Nodes (5): capacity_error(), plan(), removal(), removal_cells(), removal_refunds()

### Community 170 - "UiFrame<'_>"
Cohesion: 0.11
Nodes (18): SlotFilter, draw(), footer(), GOLD, header(), inventory(), machine(), MUTED (+10 more)

### Community 171 - "drops/queries.rs"
Cohesion: 0.22
Nodes (17): age_ms_now(), airborne_count(), capture_nearby(), collect_in_aabb(), distance_sq(), extractable(), has_expired(), live_drop() (+9 more)

### Community 172 - "decode_transaction"
Cohesion: 0.23
Nodes (9): decode_transaction(), encode_frame(), frame_checksum(), invalid_data_owned(), invalid_input(), Reader, Reader<'a>, validate_key() (+1 more)

### Community 173 - "effects/registered.rs"
Cohesion: 0.12
Nodes (13): MAX_EFFECT_BATCH_PAYLOAD_BYTES, MAX_EFFECT_BUFFER_PAYLOAD_BYTES, MAX_EFFECT_DESTINATIONS, MAX_EFFECT_KIND_ID_BYTES, MAX_EFFECT_KIND_PAYLOAD_BYTES, MAX_REGISTERED_EFFECT_KINDS, RegisteredEffectBuffer, RegisteredEffectIntent (+5 more)

### Community 174 - "gameplay/decisions.rs"
Cohesion: 0.12
Nodes (12): BLOCK_REGISTER, BLOCK_SOURCE, ByteState, ENTITY_REGISTER, ENTITY_SOURCE, entity_target_action_is_discovered_in_verified_session_catalog(), Fixture, luau_decisions_block_events_caught_error_rollback_and_restart() (+4 more)

### Community 175 - "Adapter"
Cohesion: 0.18
Nodes (3): Adapter, offset(), register()

### Community 176 - "handles.rs"
Cohesion: 0.20
Nodes (11): entity(), entity_value(), EntityId, identity_methods(), intern(), profile(), profile_value(), ProfileId (+3 more)

### Community 177 - "bloxgloom_host_api"
Cohesion: 0.06
Nodes (5): definition(), KEY, register(), KEY, State

### Community 178 - "systems/world.rs"
Cohesion: 0.23
Nodes (5): capture(), capture_entities(), EditInputs, plan_edits(), within_radius()

### Community 179 - "KilnPayload"
Cohesion: 0.18
Nodes (3): fuel_ticks(), KilnPayload, KilnSlot

### Community 180 - "custom.rs"
Cohesion: 0.17
Nodes (12): compose(), Descriptor, Material, MaterialSource, MAX_DESCRIPTOR_BYTES, MAX_MATERIALS, MAX_SHADER_BYTES, prepare() (+4 more)

### Community 181 - "init"
Cohesion: 0.14
Nodes (3): DEFAULT_FILTER, filter(), init()

### Community 182 - "Scripting capabilities for mod developers"
Cohesion: 0.07
Nodes (27): Actions, commands and gameplay decisions, Authored materials, effects and Luau parameters, Authoring tools and runnable examples, Client startup and authored UI, Content, Durable owner systems, Features requiring engine work or native extensions, Gameplay context services (+19 more)

### Community 183 - "content/composition.rs"
Cohesion: 0.15
Nodes (9): Catalog, Composition, field(), MAX_MEMBERS, MAX_PACKAGES, MAX_TAGS, Package, tag_kind() (+1 more)

### Community 184 - "Chunk"
Cohesion: 0.18
Nodes (10): build_bounce(), index(), is_opaque(), LightField, MAX_LIGHT, PLANE, propagate(), SIDE (+2 more)

### Community 185 - "ClientApp"
Cohesion: 0.09
Nodes (6): chunk_in_view(), ClientApp, ClientApp, lighting_depends_on(), mesh_priority(), LightSample

### Community 188 - "AnchoredBlockEntity"
Cohesion: 0.13
Nodes (3): AnchoredBlockEntity, Catalog, Catalog

### Community 189 - "Item"
Cohesion: 0.13
Nodes (17): Block, BlockState, Components, DropAnimation, .BYTE_LEN, FaceTextures, Geometry, Item (+9 more)

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
Nodes (6): Clock, Clock, DOMAIN, publish(), ReadStamp, state_key()

### Community 196 - "parallel/tests.rs"
Cohesion: 0.23
Nodes (15): batch(), bounded_queue_reports_saturation_without_accepting_a_partial_job(), cancellation_skips_queued_work_and_marks_running_results_cancelled(), completed_jobs(), dependency_waves_can_commit_twice_in_one_tick_and_later_wave_sees_prior_result(), job_errors_and_panics_reach_the_barrier_and_the_pool_keeps_running(), key(), owner_at() (+7 more)

### Community 197 - "Plan: built-in/mod capability parity"
Cohesion: 0.08
Nodes (24): 1. Establish the boundary and parity inventory, 2. Complete the container/block-entity vertical slice, 3. Complete dynamic entities and presentation, 4. Close remaining gameplay and world surfaces, 5. Prove integration outside engine internals, Adopted direction, Client presentation and resources, Completed task: player-response path hardening (+16 more)

### Community 198 - "sync"
Cohesion: 0.12
Nodes (7): channel(), Sender, Shared, State, background_backlog_cannot_fill_edit_capacity_and_invalidation_removes_queued_work(), hot_edit_coalesces_and_promotes_without_losing_background_progress(), shutdown_wakes_idle_workers()

### Community 199 - "material.rs"
Cohesion: 0.19
Nodes (15): blend_opposite_pixels(), emission_strengths(), face_uv(), item_material_layer(), item_material_layer_for(), material_layer(), material_layer_for(), material_mips() (+7 more)

### Community 200 - "Adapter"
Cohesion: 0.16
Nodes (4): Adapter, error(), register(), spawn_clear()

### Community 201 - "script/gameplay.rs"
Cohesion: 0.12
Nodes (6): command_declaration(), command_schema(), Declaration, declarer(), registration(), ScriptHandler

### Community 202 - "Runtime"
Cohesion: 0.16
Nodes (4): count(), number(), own_key(), Runtime

### Community 203 - "declarer"
Cohesion: 0.27
Nodes (4): declarer(), field(), number(), triple()

### Community 205 - "Complete modding implementation proposal"
Cohesion: 0.07
Nodes (28): 10. Custom models: explicitly deferred, 11. Developer workflow and maintenance rules, 13. Completion and verification, 14. Continuation record — update during implementation, 1. What approval means, 2. Product outcome, 3. Starting point: preserve the useful work, 4. One coherent gameplay surface (+20 more)

### Community 206 - "Update"
Cohesion: 0.21
Nodes (7): defaults(), Definition, identifier(), Kind, MAX_PARAMETERS, State, Update

### Community 207 - "client/startup.rs"
Cohesion: 0.17
Nodes (10): ascii(), display(), execute(), identifier(), identity(), load(), prepare(), validate_sources() (+2 more)

### Community 208 - "VisualFire"
Cohesion: 0.09
Nodes (12): EffectBuffer, LIFE, MAX_EMBERS, FireRenderer, FireStyle, FLOATS, MAX_BYTES, MAX_FIRES (+4 more)

### Community 209 - "render/tests.rs"
Cohesion: 0.11
Nodes (8): mesh_chunk(), adjacent_leaves_skip_interior_cutout_faces(), grass_side_is_upright_on_both_wall_axes(), greedy_quads_repeat_material_once_per_voxel(), mapped_builtin(), meshing_uses_shared_chunk_layout_and_world_origin(), plants_have_two_crossed_cutout_quads_and_do_not_hide_ground(), remapped_connection_catalog_drives_foliage_meshes_and_drop_art()

### Community 210 - "machine_component_tests.rs"
Cohesion: 0.19
Nodes (9): exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers(), independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart(), load_neighbours(), payload(), public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports(), pulse(), SelectiveMachine, settle() (+1 more)

### Community 211 - "EffectKindId"
Cohesion: 0.17
Nodes (4): EffectKindId, EffectKindRegistry, EffectKindRegistryFrozen, EffectRegistryError

### Community 212 - "view"
Cohesion: 0.11
Nodes (17): Mossbun, BODY, gravity_accelerates_and_sweeps_to_exact_landing_without_tunneling(), ground_motion_respects_walls_cliffs_seams_and_embedded_edits(), view(), BODY, register(), a_new_obstacle_invalidates_the_current_waypoint_before_movement() (+9 more)

### Community 213 - "extension_lifecycle.rs"
Cohesion: 0.32
Nodes (8): Blocks, inventories, and processing, external_owner_world_read_survives_real_listener_join_and_restart(), external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp(), external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener(), registered_item_components_transfer_and_recover_over_real_listener(), send(), storage_roundtrip(), until()

### Community 214 - "SystemDescriptor"
Cohesion: 0.05
Nodes (30): access(), AccessKind, BudgetKind, depends_on(), ExecutableSystem, F, IdentifierError, MAX_DEPENDENCIES_PER_SYSTEM (+22 more)

### Community 215 - "neighborhood.rs"
Cohesion: 0.16
Nodes (16): luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect(), luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes(), luau_neighborhood_radius_requires_exact_bounds_and_world_capability(), fixture(), GROW, id(), inbox(), load_neighborhoods() (+8 more)

### Community 216 - "Proposal: one coherent gameplay API"
Cohesion: 0.11
Nodes (19): 10. Custom models: required direction, deferred work, 1. A small, powerful set of concepts, 2. Atomic operations are a convenience, not a burden, 3. Specialized APIs are optional conveniences, 4. Built-in gameplay uses the same boundary, 5. Distinguish genuinely different execution contexts, 6. Developer experience is part of the implementation, 7. Server-delivered mod packages (+11 more)

### Community 217 - "CoordinatorContext"
Cohesion: 0.21
Nodes (14): durable_actions(), fire_delivery(), fire_source(), input_authorization(), interaction_commit(), player_movement(), publish(), apply_simulation_input() (+6 more)

### Community 218 - "owner_durable/tests.rs"
Cohesion: 0.24
Nodes (16): byte_bound_is_enforced_at_insert_and_at_prepare_without_truncation(), capacity_defers_while_corruption_stops(), chunk(), commit_after_a_concurrent_wave_rejects_whole_and_applies_nothing(), committed_waves_mark_active_and_update_the_due_index(), counter_store(), dropped_prepared_wave_changes_nothing(), fed_deadlines_are_not_duplicated_and_replay_clears_stale_readiness() (+8 more)

### Community 219 - "StorageBlockEntity"
Cohesion: 0.23
Nodes (7): MAX_FOOTPRINT, MAX_STORAGE_SLOTS, PlacementContext, PlaceStorage, RemovalContext, RemoveStorage, StorageBlockEntity

### Community 220 - "Extension"
Cohesion: 0.13
Nodes (11): Extension, Fixture, KEY, TallStore, HarvestExtension, MarkerState, NoPickupExtension, PlacementExtension (+3 more)

### Community 221 - "wake.rs"
Cohesion: 0.14
Nodes (11): blocked(), canonical_wakes(), EntityWake, interact_producer(), INTERACT_PRODUCER_ID, MAX_WAKES_PER_PLAN, register_wake_kind(), route_wakes() (+3 more)

### Community 222 - "VisualAvatar"
Cohesion: 0.07
Nodes (17): ActorAnimator, DELAY, Sample, STEP, avatar(), interpolation_moves_between_samples_and_freezes_without_extrapolation(), landing_animation_follows_delayed_ground_contact_and_is_visual_only(), teleport_despawn_and_reappearance_reset_history() (+9 more)

### Community 223 - "Bindings"
Cohesion: 0.23
Nodes (7): Action, allowed_key(), Bindings, letter(), NamedBindings, parse(), valid_action_key()

### Community 224 - "ContentManifest"
Cohesion: 0.20
Nodes (8): checksum(), ContentEntry, ContentManifest, invalid(), MAGIC, MAX_ENTRIES, MAX_MANIFEST_BYTES, VERSION

### Community 225 - "Preparation"
Cohesion: 0.17
Nodes (3): Preparation, Ready, Renderer

### Community 226 - "prepare"
Cohesion: 0.18
Nodes (6): decode_motion_id(), invalid_data(), invalid_entity(), prepare(), PreparedEntityRecovery, validate_checkpointed_motion()

### Community 229 - "position_store.rs"
Cohesion: 0.19
Nodes (9): checksum(), invalid(), LEN, MAGIC, PositionStore, TEMP_SEQUENCE, validate_position(), validate_profile() (+1 more)

### Community 230 - "Value"
Cohesion: 0.08
Nodes (19): FootprintCell, Value, coordinate(), guarded(), integer_cell(), invoke(), LifecycleRequest, parse_lifecycle() (+11 more)

### Community 232 - "Context<'_>"
Cohesion: 0.31
Nodes (4): Context<'_>, Entity, EntityChange, EntitySpawn

### Community 233 - "fs"
Cohesion: 0.17
Nodes (10): checkpoint_keys_turn(), decode_chunk_checkpoint_key(), decode_inventory_checkpoint_key(), fire_batch_key(), is_fire_checkpoint_key(), process_checkpoint_receipts(), submit_dirty_checkpoints(), submit_fire_checkpoint_batch() (+2 more)

### Community 234 - "bounded.rs"
Cohesion: 0.18
Nodes (6): expiry_due_prefix_is_capped_and_uncommitted_work_remains_eligible(), fill_chunk(), motion_is_scheduled_entity_work_never_coordinator_stepping(), one_moving_drop_costs_its_own_records_never_the_population(), staged_motion_bytes(), test_store()

### Community 235 - "script_startup/appearance.rs"
Cohesion: 0.17
Nodes (6): appearance_bundle_composes_with_rules_animation_and_exact_world_identity(), CALL, invalid_or_caught_appearance_declarations_never_publish(), Peer, registered_appearance_selection_replicates_and_restarts_by_profile(), source()

### Community 236 - "pipeline.rs"
Cohesion: 0.22
Nodes (6): create_custom_voxel_pipeline(), create_voxel_pipeline(), create_voxel_pipeline_source(), create_voxel_pipeline_with_catalog(), SHADER, VERTEX_STRIDE

### Community 237 - ".handle"
Cohesion: 0.19
Nodes (3): DropStack, AnchorExtension, withdraw()

### Community 238 - "entity_sleep.rs"
Cohesion: 0.34
Nodes (18): queue_interaction_actions(), dispatch(), edit(), empty_action(), live_harvest_receipt_invalidates_sleeping_support_without_notification_delivery(), new_sleepers_cannot_extend_the_current_recheck_pass(), position(), receive() (+10 more)

### Community 239 - "EntityDependencies"
Cohesion: 0.15
Nodes (3): EntityDependencies, EntityStore, PreparedEntityTransaction

### Community 240 - "Adapter"
Cohesion: 0.20
Nodes (3): OwnedEntity, Adapter, OwnerWorldView

### Community 242 - "inventory"
Cohesion: 0.12
Nodes (8): register(), CONTENT, Fixture, luau_failed_restart_leaves_existing_save_unchanged_and_can_retry(), luau_startup_item_reaches_listener_inventory_and_restart(), luau_startup_rejections_publish_nothing_and_never_open_world(), luau_startup_validates_contracts_and_runs_imports_at_the_item_bound(), TOKEN

### Community 243 - "script_startup/bundle.rs"
Cohesion: 0.24
Nodes (15): bundle_gate_rejects_mismatches_and_early_play_without_blocking_healthy_join(), bundle_restart_exact_cache_and_changed_source_require_new_bytes(), client_verification_rejects_relay_tamper_truncation_and_reordering(), closed(), fixture(), fragmented(), offer(), package_effect_is_prepared_by_real_client_join_before_welcome() (+7 more)

### Community 244 - "FirePending"
Cohesion: 0.12
Nodes (12): checked_body(), checksum(), finish(), key_bytes(), read_key(), decode_pending_key(), FireIgnition, FireIgnitionId (+4 more)

### Community 245 - "script/generation.rs"
Cohesion: 0.17
Nodes (7): coordinate(), Declaration, declarer(), invoke(), registration(), runtime(), ScriptContributor

### Community 246 - "Startup"
Cohesion: 0.12
Nodes (3): ClientBundle, Format, Startup

### Community 247 - "authored.rs"
Cohesion: 0.13
Nodes (7): ATLAS_SIZE, Document, INVALID, json(), MAX_TEXT, Resources, Widget

### Community 248 - "src/composition.rs"
Cohesion: 0.12
Nodes (13): ACTIONS, ANCHORED_ENTITIES, Bundle, CONTENT, Dependency, GENERATION, INVENTORY_SCREENS, ITEM_ICONS (+5 more)

### Community 250 - "io"
Cohesion: 0.07
Nodes (10): animate(), TURN_ENTRIES, WRITE_BYTES, invalid(), place(), remove(), spawn_effects(), MAX_APPLY_JOBS_PER_BARRIER (+2 more)

### Community 251 - "client/lifecycle/tests.rs"
Cohesion: 0.35
Nodes (5): exercise_join_lifecycle(), join(), read(), retired(), slow_attempt_can_be_cancelled()

### Community 252 - "entities"
Cohesion: 0.18
Nodes (7): unrecoverable_anchored_outputs_are_rejected_before_admission_and_valid_state_recovers(), live_command(), missing_mossbun_terrain_defers_locally_then_runs_on_residency(), mossbun_authorized_spawn_worker_steps_and_restart_preserve_identity(), mossbun_spawn_limit_rejects_without_allocating_or_consuming_items(), resident_platform(), pickup_and_entity_backlogs_leave_command_room_and_one_actor_cannot_fill_it()

### Community 253 - "model.rs"
Cohesion: 0.16
Nodes (10): FUEL_SLOT_INDEX, INPUT_SLOT_INDEX, KILN_MAX_COOK_TICKS, KILN_MAX_FUEL_TICKS, KILN_MAX_PAYLOAD_BYTES, KILN_MAX_RECIPES, KILN_TICK_INTERVAL, KilnRecipe (+2 more)

### Community 254 - "package/tests.rs"
Cohesion: 0.27
Nodes (11): cycles_depth_and_shared_execution_budget_are_bounded(), dependency_versions_and_manifest_declarations_are_strict(), discovery_bounds_directory_count_source_bytes_and_total_bytes(), discovery_rejects_symlinks_at_every_path_level_and_special_files(), failed_modules_are_not_reinitialized_when_caught(), Fixture, import_failures_name_package_version_and_module_and_do_not_poison_worker(), imported_source_limits_and_exported_function_errors_keep_source_identity() (+3 more)

### Community 255 - "seed"
Cohesion: 0.29
Nodes (8): entity(), hex_id(), partition(), present(), profile(), seed(), word(), words()

### Community 257 - "slot.rs"
Cohesion: 0.12
Nodes (6): draw(), draw(), color_from_swatch(), paint_icon(), show(), SlotStyle

### Community 258 - "invoke"
Cohesion: 0.19
Nodes (7): Capabilities, captured_entity(), checked(), entity_identity(), invoke(), push_wake(), read_block()

### Community 259 - "items.rs"
Cohesion: 0.30
Nodes (8): only_block_items_are_placeable(), placeable_block(), placeable_block_in(), SAPLING, SEEDS, STICK, valid_item(), valid_item_in()

### Community 260 - "DroppedItem"
Cohesion: 0.31
Nodes (5): DropAnimator, live_visual(), PickupFlight, POSITION_BLEND, DroppedItem

### Community 262 - "Gpu"
Cohesion: 0.09
Nodes (9): AvatarMesh, AvatarVertex, build(), emit_cuboid(), humanoid_mesh_has_bounded_closed_cuboids_and_face_details(), Data, Gpu, MaterialData (+1 more)

### Community 263 - "streaming.rs"
Cohesion: 0.15
Nodes (13): OutboundClientSnapshot, can_stream_snapshot(), can_stream_snapshot_size(), inside_view(), MAX_LOAD_RESULTS_PER_TICK, MAX_NEW_LOADS_PER_CLIENT, MAX_PREFETCH_CANDIDATES, poll_chunk_loads() (+5 more)

### Community 264 - "GpuPass"
Cohesion: 0.12
Nodes (3): Data, GpuPass, texture_entry()

### Community 265 - "Hit"
Cohesion: 0.09
Nodes (27): no_hit(), no_interact(), player_adapter(), PLAYER_ENTITY_TYPE, project_avatar(), INSERT_FUEL, INSERT_INPUT, interact_verb() (+19 more)

### Community 266 - "system/intents.rs"
Cohesion: 0.18
Nodes (12): luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id(), luau_intent_send_requires_opt_in_and_absence_requires_bootstrap(), CHAIN, id(), inbox(), luau_intent_absent_destinations_run_on_real_listener_and_recover_once(), luau_intent_caught_invalid_and_overbudget_sends_poison_all_output(), luau_intent_declarations_and_session_identity_are_owned_and_bounded() (+4 more)

### Community 267 - "journal/recovery.rs"
Cohesion: 0.30
Nodes (7): Journal, legacy_header(), open_or_create_legacy(), sync_parent(), truncate_tail(), validate_legacy_header(), write_legacy_header()

### Community 268 - "client/world.rs"
Cohesion: 0.22
Nodes (7): Cursor, MAX_PENDING_GROUPS, MAX_PENDING_SNAPSHOTS, PendingCommit, PendingSnapshot, WorldProbe, WorldUpdate

### Community 269 - "registry/tests.rs"
Cohesion: 0.19
Nodes (19): builtin_phase_plan(), register_builtin_systems(), declarations_match_the_current_execution_shape(), descriptor(), deterministic_plan(), disjoint_and_read_only_accesses_can_share_a_phase(), duplicate_ids_and_invalid_namespaced_ids_are_rejected(), freeze_is_registration_order_independent_and_accepts_transitive_conflict_order() (+11 more)

### Community 271 - "Imports"
Cohesion: 0.17
Nodes (3): Imports, MAX_IMPORT_DEPTH, ModuleState

### Community 272 - "OwnerApplyReceipt"
Cohesion: 0.23
Nodes (3): OwnerApplyReceipt, OwnerApplyTask, World

### Community 274 - "transfer.rs"
Cohesion: 0.17
Nodes (7): AutomationStack, EntityItemTransfer, movable_count(), move_up_to(), PortRoute, put(), take()

### Community 275 - "package/manifest.rs"
Cohesion: 0.18
Nodes (8): asset_path(), bounded_path(), identifier(), Manifest, public_path(), SourceSide, valid_path(), valid_version()

### Community 276 - ".register_action"
Cohesion: 0.08
Nodes (3): Catalog, Catalog, Catalog

### Community 277 - "server/appearance.rs"
Cohesion: 0.23
Nodes (5): checksum(), LEN, select(), SEQUENCE, Store

### Community 278 - "anchored_tests.rs"
Cohesion: 0.16
Nodes (14): anchored_custom_state_cost_use_neighbor_support_and_recovery_are_atomic(), command(), edit(), fire_invalidates_two_cross_chunk_footprints_with_refunds_in_one_wal_record(), KEY, open(), public(), resident() (+6 more)

### Community 280 - "burn.rs"
Cohesion: 0.24
Nodes (9): assert_burned(), assert_uncommitted(), burn_startup(), definition(), destination(), plant_burn(), public_owner_burn_declaration_requires_terrain_and_changes_only_opt_in_fingerprint(), public_owner_burn_rejects_replacement_without_committing_any_participant() (+1 more)

### Community 282 - ".public_view"
Cohesion: 0.35
Nodes (4): AppearanceCodec, encode_stack(), StackPayload, StackPayloadCodec

### Community 286 - "EffectConsumerOutput"
Cohesion: 0.29
Nodes (3): EffectConsumerOutput, RegisteredEffectLimits, PatchUsage

### Community 288 - "notifications.rs"
Cohesion: 0.23
Nodes (4): Lane, MAX_EVENT_BYTES, QUEUE, view_entity()

### Community 289 - "server/checkpoint/tests.rs"
Cohesion: 0.27
Nodes (14): capacity_counts_running_jobs_and_unconsumed_receipts(), closure_panic_becomes_an_error_receipt_and_worker_keeps_running(), completed_but_unconsumed_receipt_still_occupies_capacity(), drop_drains_accepted_work_without_blocking_on_full_receipt_channel(), failed_write_is_returned_with_its_key_and_revision(), independent_checkpoint_keys_progress_while_another_shard_is_blocked(), key(), key_on_shard() (+6 more)

### Community 290 - "visibility.rs"
Cohesion: 0.15
Nodes (6): Atmosphere, smooth(), chunk_visible(), chunk_visible_padded(), outside_clip(), view_projection()

### Community 291 - "bundle_ui.rs"
Cohesion: 0.09
Nodes (18): event(), client_startup_failure_refuses_content_ready_with_package_and_module(), downloaded_client_startup_is_session_scoped_across_reconnect_and_switch(), downloaded_replica_visuals_use_exact_entity_ids_and_reset_on_switch(), startup_fixture(), startup_worker_discards_partial_registration_and_caught_limit(), startup_worker_imports_exact_direct_dependencies_with_lexical_visibility(), verified_replica_callbacks_are_session_scoped_worker_presentations() (+10 more)

### Community 292 - "server/gameplay/entities.rs"
Cohesion: 0.40
Nodes (7): anchored(), nearby(), project(), read(), state(), validate_owner(), validate_state()

### Community 293 - "ItemIcon"
Cohesion: 0.14
Nodes (3): ItemIcon, definitions(), Catalog

### Community 295 - ".window_event"
Cohesion: 0.09
Nodes (3): escape_screen(), ClientApp, inventory_screen()

### Community 296 - "tick/tests.rs"
Cohesion: 0.31
Nodes (10): drop_snapshot(), dropped_column_lands_at_rest_and_suspends(), falling_drop_integrates_exactly_one_fixed_step(), missing_terrain_defers_fail_closed_without_guessing(), neighbourhood_view(), neighbours(), NEXT_TEST_DIR, planner_rejects_anchored_locations_and_foreign_payloads() (+2 more)

### Community 297 - "entities/container.rs"
Cohesion: 0.27
Nodes (3): Codec, ContainerPayload, register()

### Community 298 - "script_startup/machine.rs"
Cohesion: 0.12
Nodes (18): luau_component_machine_processes_exact_stack_over_listener_and_recovers(), luau_machine_component_options_reject_invalid_constants_before_save(), luau_machine_component_recipe_negotiates_exact_predicate_and_preservation(), luau_machine_present_input_exact_output_and_component_fuel_roundtrip(), source(), luau_machine_footprint_negotiates_across_seam_and_restarts(), luau_machine_footprint_places_and_breaks_secondary_cell_over_listener(), luau_machine_negotiates_plans_and_restarts() (+10 more)

### Community 299 - "Behavior"
Cohesion: 0.22
Nodes (4): Behavior, DownwardFlow, Plan, Processor

### Community 301 - "Inputs"
Cohesion: 0.16
Nodes (3): expand(), Expansion, Inputs

### Community 302 - ".draw_node"
Cohesion: 0.22
Nodes (3): Intent, rgba(), Session

### Community 303 - "admin/tests.rs"
Cohesion: 0.14
Nodes (3): generic_parser_uses_ordered_negotiated_schema_not_builtin_names(), request(), command_signature()

### Community 304 - "server/perf.rs"
Cohesion: 0.20
Nodes (10): PHASE_NAMES, print_latency(), print_metric(), print_report(), run_fire_cpu_perf(), run_fire_perf(), run_perf_benchmark(), run_scenario() (+2 more)

### Community 305 - "Downloads"
Cohesion: 0.10
Nodes (4): complete_package_handshake(), Downloads, receive_package(), state_with_package()

### Community 306 - "receive_content_manifest"
Cohesion: 0.09
Nodes (19): client_commands_are_rejected_until_content_ready_matches(), complete_content_handshake(), clock(), external_system_runs_without_entities_and_recovers_across_real_listener_restart(), startup(), join_cleanup_enqueues_one_leave_with_the_next_sequence(), local_server_shutdown_restores_authoritative_position_on_next_start(), nonblocking_listener_streams_and_recovers_a_wal_acked_edit_inner() (+11 more)

### Community 308 - "Input"
Cohesion: 0.24
Nodes (6): apply(), disconnect(), Input, prepare(), Prepared, reduce_view_under_pressure()

### Community 309 - "inventory/container.rs"
Cohesion: 0.24
Nodes (5): decode(), encode(), invalid(), max_bytes(), independent_container_codec_roundtrips_more_than_backpack_and_rejects_noncanonical_data()

### Community 311 - "combined.rs"
Cohesion: 0.24
Nodes (7): AIM, combined_mod_downloads_acts_grows_and_recovers(), combined_mod_two_profiles_act_independently_and_recover(), GROW, grow_once(), open(), PROFILE

### Community 312 - "run_loop"
Cohesion: 0.35
Nodes (3): apply_motion(), run(), run_loop()

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

### Community 319 - "route_registered_effects"
Cohesion: 0.33
Nodes (11): route_registered_effects(), chunk(), emitted(), invalid_payload_and_duplicate_destinations_are_rejected(), keys(), oversized_expanded_fanout_aborts_routing_before_any_batch_is_returned(), registered_consumer_builds_a_typed_scratch_patch_for_its_destination(), registry() (+3 more)

### Community 320 - "script_startup/gameplay/entities.rs"
Cohesion: 0.19
Nodes (7): Fixture, luau_entity_schema_loopback_spawn_due_callback_and_recovery(), luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling(), prepare(), REGISTER, REGISTER_ENTITY, SOURCE

### Community 321 - "script_startup/gameplay/inventory.rs"
Cohesion: 0.21
Nodes (10): actor(), luau_automatic_pickup_exact_components_conservation_and_restart(), luau_component_schema_rejects_wrong_payload_and_persists_exact_bytes(), luau_inventory_exact_binary_reads_give_take_and_error_rollback_restart(), luau_pickup_host_credit_eligibility_permissions_and_caught_errors_rollback(), luau_take_and_spawn_stack_preserve_binary_components_across_receipt_and_restart(), PICKUP, PICKUP_REGISTER (+2 more)

### Community 322 - "Harvest"
Cohesion: 0.33
Nodes (3): Harvest, Pickup, PlantSupport

### Community 323 - "position_store/tests.rs"
Cohesion: 0.21
Nodes (5): corrupted_position_is_not_silently_replaced(), position_checkpoint_does_not_share_inventory_temp_namespace(), position_round_trips_and_is_profile_scoped(), SEQUENCE, TestSave

### Community 325 - "complete_barrier"
Cohesion: 0.35
Nodes (6): advance_receipts(), CommitProgress, complete_barrier(), drain_staged_receipts(), flush_ready_fire(), poll_journal_receipts()

### Community 326 - "FireAnimator"
Cohesion: 0.23
Nodes (4): FireAnimator, LIFE, MAX_DISTANCE_SQUARED, MAX_FIRES

### Community 327 - "Catalog"
Cohesion: 0.09
Nodes (20): DropSize, BlockDef, BlockTextures, Catalog, checked_id(), EntityTypeDef, fingerprint_texture(), flags() (+12 more)

### Community 328 - "queries/tests.rs"
Cohesion: 0.33
Nodes (7): airborne_count_tracks_schedule_not_records(), expired_drop_is_visible_but_never_pickable(), immutable_mobile_pages_keep_old_capture_and_project_nearest_without_full_output_allocation(), pickup_delay_gates_candidates_but_not_visibility(), single_drop_collection_rechecks_range_delay_and_expiry(), spawn_direct(), test_store()

### Community 329 - "tests/anchored.rs"
Cohesion: 0.32
Nodes (7): owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record(), owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal(), removed(), seed_storage(), storage_startup(), STORE, unchanged()

### Community 331 - "key"
Cohesion: 0.32
Nodes (5): Command, CommandArgument, CommandValue, MAX_COMMAND_ARGUMENTS, key()

### Community 332 - ".resolve"
Cohesion: 0.18
Nodes (8): identifier(), owned(), bounded_nodes(), Kind, Presentation, RawDocument, RawNode, Style

### Community 333 - "prepare"
Cohesion: 0.29
Nodes (3): accepts(), capture(), prepare()

### Community 334 - "script_startup/generation.rs"
Cohesion: 0.24
Nodes (8): Fixture, GENERATION, luau_generation_baseline_edits_and_identity_survive_restart(), luau_generation_rejects_bad_registration_and_caught_output_errors(), luau_generation_sampling_is_exact_frozen_and_fresh_across_parallel_loads(), luau_generation_streams_from_loader_after_restart(), MARKER, REGISTER

### Community 335 - "install"
Cohesion: 0.27
Nodes (8): amount(), decode_stack(), field(), index(), install(), latch(), owner(), stack_table()

### Community 336 - "declarer"
Cohesion: 0.11
Nodes (6): Declaration, declarer(), field(), parse_recipe(), Declaration, declarer()

### Community 337 - "streaming/entities.rs"
Cohesion: 0.19
Nodes (5): MAX_PUBLIC_ENTITIES_PER_CHUNK, MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK, project(), snapshot_messages(), SnapshotError

### Community 338 - "publication.rs"
Cohesion: 0.33
Nodes (7): apply_committed_action(), apply_committed_action_inner(), apply_committed_fire_action(), apply_committed_owner_world(), is_owner_publication_key(), publish_committed(), publish_committed_fire_after_world()

### Community 339 - "src/actions/tests.rs"
Cohesion: 0.24
Nodes (6): action(), command_facets_are_empty_gameplay_only_and_fingerprint_permissions(), composed_controls_resolve_forward_references_without_changing_target_context(), composition_bounds_and_fingerprint_cover_every_control(), discovery_is_bounded_ordered_and_rejects_conflicting_ownership(), ordered_command_schema_has_canonical_bounded_arguments_and_identity()

### Community 341 - "lifecycle-fixture/src/machine.rs"
Cohesion: 0.20
Nodes (5): Crush, KEY, MARKED_INPUT, REFINED_INPUT, register()

### Community 343 - "Bloxgloom interface plan"
Cohesion: 0.22
Nodes (8): Baseline when this plan was written, Bloxgloom interface plan, Delivery order, Goal, Interaction contract, Performance and correctness, UI and game-state design, Validation record

### Community 344 - "entity"
Cohesion: 0.25
Nodes (4): definition(), KEY, State, definition()

### Community 345 - "DropPolicy"
Cohesion: 0.15
Nodes (7): DropPolicy, .BYTE_LEN, .MAX_PICKUP_RANGE, component_schema(), drop_policy(), options(), hexadecimal_schema_fingerprint_is_exact_and_cannot_mix_encodings()

### Community 346 - "Remaining work by phase (living checklist)"
Cohesion: 0.18
Nodes (11): 12. Implementation order and deliverables, Explicitly deferred outside the phases, Phase 1 — shared core · Done, Phase 2 — built-in gameplay parity · Done (native fire deferred), Phase 3 — generation · Done, Phase 4 — Luau authoring · Done, Phase 5 — packages and joining · Done, Phase 6 — authored UI · Done (+3 more)

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

### Community 354 - "scheduling_tests.rs"
Cohesion: 0.27
Nodes (7): due_reschedule_waits_for_receipt_and_deferral_preserves_eligibility(), increment(), invalid_handler_deadline_rejects_without_losing_due_work(), multi_job_due_dispatch_and_restart_keep_deadlines_and_rotation(), run(), scheduled_startup(), wake_only_turns_without_runnable_work_preserve_ordinary_rotation()

### Community 355 - "Bloxgloom"
Cohesion: 0.22
Nodes (9): Bloxgloom, Client execution and remaining extension work, Current execution architecture, Development and previews, HDR presentation, Publication and checkpoint boundaries, Run locally, Server threads and workers (+1 more)

### Community 356 - "chunk_loader/tests.rs"
Cohesion: 0.31
Nodes (7): accepted_work_budget_has_explicit_nonblocking_overflow(), pre_edit_worker_result_is_rejected_after_uncheckpointed_edit(), receive_before(), requests_are_deduplicated_and_negative_chunks_load_asynchronously(), test_dir(), TEST_DIR_COUNTER, worker_load_uses_uncheckpointed_authoritative_snapshot_after_eviction()

### Community 357 - "drop_merge.rs"
Cohesion: 0.24
Nodes (4): DropMergeCandidate, DropMergeContext, DropStackFill, filling_and_splitting_conserve_items_at_the_stack_cap()

### Community 359 - "coordinator/tests.rs"
Cohesion: 0.40
Nodes (6): acknowledged_result_stays_retired_across_rotation_and_restart(), grant(), inventory_action(), poll_until_settled(), temp_save_dir(), wal_replay_keeps_result_and_world_effect_before_checkpoint()

### Community 360 - "startup/block.rs"
Cohesion: 0.33
Nodes (7): boolean(), cube(), extended(), has_state(), placement_state(), stateful(), visual()

### Community 361 - "journal.rs"
Cohesion: 0.08
Nodes (16): CLOCK_DOMAIN, FILE_HEADER_LEN, FILE_MAGIC, FILE_VERSION, FRAME_OVERHEAD, JOURNAL_ROTATION_SOFT_LIMIT_BYTES, MAX_BATCH_RECORDS, MAX_CHANGES (+8 more)

### Community 362 - "refund_stacks"
Cohesion: 0.21
Nodes (4): place(), refund_removal(), refund_stacks(), remove()

### Community 365 - "decode"
Cohesion: 0.27
Nodes (3): decode(), encode(), Format

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

### Community 374 - "coder-fast.md"
Cohesion: 0.25
Nodes (7): Commits, Concurrency, Report when done, Scope discipline, Startup, Tests, Verification

### Community 375 - "fields"
Cohesion: 0.33
Nodes (4): block(), fields(), every_public_removal_cause_preserves_its_exact_luau_context(), triple()

### Community 380 - "visual_contracts.rs"
Cohesion: 0.21
Nodes (8): decode(), copy(), fixture(), installed_authoritative_entity_replica_drives_the_shader_parameter(), invalid_typed_startup_parameter_refuses_real_join_with_source_identity(), negotiated_visuals_parameters_switch_and_restart_without_global_state(), open_boxed(), scalar()

### Community 382 - "spawn.rs"
Cohesion: 0.49
Nodes (5): collides(), collides_cached(), request_missing(), spawn_position(), spawn_position_cached()

### Community 385 - "WorldTime"
Cohesion: 0.36
Nodes (3): Context<'_>, WorldTime, Capture

### Community 389 - "render.rs"
Cohesion: 0.06
Nodes (15): Camera, DEPTH_FORMAT, MAX_PENDING_MESHES, RendererError, RenderStats, SKY_COLOR, sky_camera_data(), SKY_SHADER (+7 more)

### Community 390 - "config/tests.rs"
Cohesion: 0.39
Nodes (6): binding_conflicts_and_movement_keys_fail_back_to_defaults(), config_round_trips_through_explicit_path(), invalid_values_are_clamped_and_corrupt_files_fall_back(), profile_is_generated_once_and_survives_reload(), saving_sanitizes_public_values(), test_directory()

### Community 392 - "resolve_player_movement"
Cohesion: 0.42
Nodes (4): MAX_MOVEMENT_STEPS_PER_AXIS, player_collides(), resolve_player_movement(), ResolveError

### Community 393 - "avatars/tests.rs"
Cohesion: 0.28
Nodes (4): gpu_registered_player_palettes_preserve_default_and_color_all_three_parts(), HEIGHT, render(), WIDTH

### Community 394 - "register"
Cohesion: 0.31
Nodes (3): Chest, definition(), register()

### Community 396 - "server/entities/kiln.rs"
Cohesion: 0.23
Nodes (6): kiln_block_states(), kiln_payload(), kiln_state(), KilnHalf, register_entity_type(), register_entity_type_with_recipes()

### Community 398 - "decode"
Cohesion: 0.36
Nodes (3): decode(), encode(), property_identifier()

### Community 403 - "parallel.rs"
Cohesion: 0.11
Nodes (11): spread_over_tcp(), MAX_PHASE_QUEUE_CAPACITY, MAX_PHASE_RESULT_CAPACITY, MAX_PHASE_WORKERS, panic_message(), effect_patches_count_emissions_and_expose_their_replacement(), foreign_payloads_have_no_replacement(), plain_patches_carry_no_emissions_and_keep_their_replacement() (+3 more)

### Community 405 - "outbound/tests.rs"
Cohesion: 0.43
Nodes (6): aggregate_byte_limit_is_enforced_across_clients(), aggregate_high_water_mark_survives_sub_tick_queue_drain(), frame_admission_includes_the_frame_currently_being_written(), per_client_byte_limit_is_shared_by_queue_clones_and_released_on_drop(), pong(), shared_encoding_keeps_independent_byte_reservations_until_each_client_releases()

### Community 407 - "render/effects/tests.rs"
Cohesion: 0.32
Nodes (4): gpu_preview(), SHADER, verified_example_gpu_pass_survives_resize_and_grades_scene(), version_two_graph_composes_declared_inputs_and_parameters_on_gpu()

### Community 409 - "transfer"
Cohesion: 0.33
Nodes (3): Work, parse(), transfer()

### Community 410 - "Public dynamic-entity surface"
Cohesion: 0.10
Nodes (18): Behavior and movement, Bounds and remaining surfaces, External proof: Copperling, Interaction and presentation, Public dynamic-entity surface, Registration and identity, Verification, Generic client and server paths (+10 more)

### Community 411 - "script/tests.rs"
Cohesion: 0.54
Nodes (6): elapsed_deadline_is_reported_with_module_identity(), input(), instruction_and_source_limits_reject_bad_modules_without_poisoning_worker(), memory_limit_and_syntax_error_are_attributable(), module(), sandbox_excludes_native_io_and_attributes_errors()

### Community 413 - "Public storage lifecycle boundary"
Cohesion: 0.33
Nodes (6): Chest integration, Explicit limits / next work, Lifecycle contract, Package boundary, Public storage lifecycle boundary, Verification

### Community 415 - "Registered content and composition"
Cohesion: 0.16
Nodes (11): Authority, scheduling and recovery, Bounds and limits, Public contract, Registered anchored behavior, Verification, Bounds, atomicity and compatibility, Composition and deterministic resolution, Existing capabilities exposed (+3 more)

### Community 416 - "Cross-cutting integration findings"
Cohesion: 0.13
Nodes (15): Approved unified gameplay implementation, Audit boundary and live path, Baseline and verdict, Built-in capability parity audit, Cross-cutting integration findings, Evidence and integration acceptance, Existing capabilities versus new gameplay, F1 — Registered actions must replace the still-live kiln shortcut (+7 more)

### Community 422 - "Registered actions and composed controls"
Cohesion: 0.33
Nodes (5): Bounded composition, Negotiated commands, Production path and authority, Registered actions and composed controls, Supported targets and effects

### Community 423 - "client/appearance.rs"
Cohesion: 0.48
Nodes (3): apply_environment(), invalid(), parse()

### Community 428 - "run"
Cohesion: 0.43
Nodes (3): main(), parse_tint(), run()

### Community 429 - "client/drops/tests.rs"
Cohesion: 0.43
Nodes (4): authored_motion_uses_server_age_and_continues_into_partial_pickup(), item(), moving_drop_blends_between_authoritative_positions(), sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion()

### Community 430 - "custom/tests.rs"
Cohesion: 0.38
Nodes (4): gpu_custom_tile_shades_only_its_layer_and_keeps_normal_geometry(), gpu_preview(), gpu_version_two_hooks_use_multiple_materials_and_runtime_parameters(), GREEN

### Community 433 - "receipts/tests.rs"
Cohesion: 0.43
Nodes (4): ack_retires_both_outcomes_without_reopening_sequence(), bounded_window_can_run_beyond_old_lifetime_cap(), reconnect_closes_old_epoch_and_preserves_monotonic_epoch_after_codec(), record()

### Community 434 - ".compose_current_package_action_with_args"
Cohesion: 0.19
Nodes (9): ActionChoice, ClientApp, compose_named_command(), compose_observed_entity_action(), compose_package_action(), compose_package_action_with_args(), PackageActionInput, authored_entity_action_uses_observed_identity_and_exact_bounded_arguments() (+1 more)

### Community 435 - "quad"
Cohesion: 0.43
Nodes (3): linear_color(), quad(), Session

### Community 436 - "write_frame"
Cohesion: 0.25
Nodes (3): bounded_turns_preserve_crc_and_stop_at_failure_without_consuming_suffix(), byte_budget_stops_turns_and_file_limit_does_not_write_overshoot(), write_frame()

### Community 441 - "system/decisions.rs"
Cohesion: 0.38
Nodes (5): caught_invalid_burn_removal_decision_rejects_whole_owner_wave(), luau_burn_removal_context_and_drop_commit_with_owner_receipt(), OWNER, package(), REGISTER

### Community 442 - "coder-smart.md"
Cohesion: 0.33
Nodes (5): Handoff, Implementation standard, Shared tree and commits, Start and scope, Verification

### Community 443 - "Phase 8: examples, parity and integrated verification"
Cohesion: 0.33
Nodes (6): Authoring and runnable examples, Integrated behavior and responsiveness, Phase 8: examples, parity and integrated verification, Production parity audit, Reproduce verification, Visual inspection and rendering context

### Community 445 - "effects/shader.rs"
Cohesion: 0.40
Nodes (3): compose(), STUBS, validate()

### Community 446 - "Agent guidance"
Cohesion: 0.40
Nodes (4): Agent guidance, Architecture and invariants, graphify, Verify graphics and performance

### Community 447 - "Authored materials and effects"
Cohesion: 0.40
Nodes (5): Authored materials and effects, Effect contract 2, Material contract 2, Preparation and compatibility, Typed values from Luau

### Community 448 - "replant.rs"
Cohesion: 0.40
Nodes (3): MAX_REPLANT_CELLS, OWNER_PROBES_PER_TICK, Replanter

### Community 449 - "passes"
Cohesion: 0.60
Nodes (3): after_dependencies_use_the_same_cycle_and_missing_checks(), inputs_override_order_and_invalid_graphs_keep_resource_identity(), passes()

### Community 452 - "Definitions, assets, and composition"
Cohesion: 0.22
Nodes (4): Capability matrix, Definitions, assets, and composition, block(), Catalog

### Community 453 - "TransferSelection"
Cohesion: 0.18
Nodes (3): StackSelector, TransferSelection, Adapter

### Community 454 - "gameplay"
Cohesion: 0.14
Nodes (5): Admin, GIVE, SPAWN, TIME, KEY

### Community 458 - "Modding: start here"
Cohesion: 0.50
Nodes (4): Historical design and audit, Modding: start here, Rust extension and host reference, Try authoring now

### Community 459 - "src/lifecycle/tests.rs"
Cohesion: 0.83
Nodes (3): storage(), storage_faces_default_to_all_cardinal_normals_and_restrict_explicitly(), storage_faces_reject_empty_duplicate_and_non_cardinal_lists()

### Community 461 - "composed.rs"
Cohesion: 0.50
Nodes (3): DECLARE, HANDLER, luau_world_entity_inventory_drop_and_schedule_are_one_retryable_receipt()

### Community 462 - "std"
Cohesion: 0.16
Nodes (13): external_anchored_initialization_use_refund_and_restart_over_real_listener(), decode_anchor(), invalid(), prepare_recovery(), read(), replay(), save(), Snapshot (+5 more)

### Community 465 - "bloxgloom"
Cohesion: 1.00
Nodes (3): bloxgloom, bloxgloom-host-api, bloxgloom-lifecycle-fixture

### Community 505 - "target_actions.rs"
Cohesion: 0.25
Nodes (4): FIRST, PROFILE, SECOND, THIRD

### Community 507 - "palette/tests.rs"
Cohesion: 0.43
Nodes (4): full_palette_reuses_removed_entry_and_preserves_other_cells(), row_copy_matches_flat_storage_in_each_mode(), state(), uniform_and_local_palette_boundaries_round_trip()

### Community 509 - ".encode_source_patches"
Cohesion: 0.20
Nodes (3): FireRuntime, source_transaction(), SourceEncodeInput

### Community 513 - "combined/mixed.rs"
Cohesion: 0.33
Nodes (4): combined_mod_mixed_load_preserves_response_progress_and_restart(), report(), ROUNDS, TARGETS

### Community 522 - "validate"
Cohesion: 0.80
Nodes (3): dword(), validate(), word()

### Community 527 - "frozen_wake_registry"
Cohesion: 0.60
Nodes (5): frozen_wake_registry(), wake_buffer_overflow_rejects_the_whole_producer_output(), wake_emit_with_unknown_kind_is_rejected(), wake_routing_is_deterministic_across_repeated_runs(), tick_producer()

## Knowledge Gaps
- **813 isolated node(s):** `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS`, `REQUEST_TAG`, `TERRAIN_REQUEST_TAG` (+808 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 3278 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **139 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Built-in capability parity audit` connect `Cross-cutting integration findings` to `Definitions, assets, and composition`, `MODDING-SURFACE-PLAN.md`?**
  _High betweenness centrality (0.058) - this node is a cross-community bridge._
- **Why does `EntityError` connect `EntityError` to `entities/persistence.rs`, `EntityStore`, `entities/types.rs`, `EntityTypeDescriptor`, `register`, `MobilePages`, `VoxelView`, `server/entities/kiln.rs`, `transfer.rs`, `drops/entity.rs`, `kiln/planning.rs`, `register`, `Port<P>`, `collections`, `plan`, `entities/container.rs`, `server.rs`, `drops/planning.rs`, `Adapter`, `KilnPayload`, `EntityPublicView`, `KilnFacing`, `ServerStartup`, `EntityTransferPolicy`, `TransferSelection`, `server/entities/tests.rs`, `Adapter`, `Adapter`, `view`, `actions/entity.rs`, `navigation.rs`, `durable/coordinator.rs`, `World`, `Adapter`, `kiln/tests.rs`, `EntityDependencies`, `MachinePayload`, `inventory`, `entities/checkpoint.rs`, `.deposit`, `model.rs`?**
  _High betweenness centrality (0.057) - this node is a cross-community bridge._
- **Why does `Audit boundary and live path` connect `Cross-cutting integration findings` to `ServerStartup`, `plan_durable_request`, `Resolved`, `Replicas`?**
  _High betweenness centrality (0.042) - this node is a cross-community bridge._
- **What connects `MAX_ACTIONS`, `MAX_TARGET_ACTIONS`, `MAX_WIDGETS` to the rest of the system?**
  _813 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `entities/persistence.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0711864406779661 - nodes in this community are weakly interconnected._
- **Should `EntityStore` be split into smaller, more focused modules?**
  _Cohesion score 0.06351678781585324 - nodes in this community are weakly interconnected._
- **Should `entities/types.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06203007518796992 - nodes in this community are weakly interconnected._