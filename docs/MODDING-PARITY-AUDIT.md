# Built-in capability parity audit

## Baseline and verdict

### Approved unified gameplay implementation

`MODDING-IMPLEMENTATION-PROPOSAL.md` is approved and is the active execution
record, including Luau/mlua and fire migration. Phase 1 now has a shared staged
terrain/drop context and startup-registered removal decision handlers. Normal
breaks, replacement/support-loss harvest and anchored placement's displaced plants
use it. Built-in harvest policy uses only the public host API. A registered
handler can read neighbouring terrain and stage extra edits plus drops in the
same command transaction, with recorded dependencies and restart recovery.
Registered container/machine inventory transfers and generic durable mobile
entities now share that atomic planning boundary. Anchor destruction also invokes
registered removal decisions while preserving footprint refunds without duplicate
cube loot. The Phase 1 gameplay transaction context is complete; Phase 2 events,
scheduled world behavior, drop policies, player rules and fire migration, plus
later Luau/package/presentation phases, remain open. This is not full parity.
Phase 2 includes registered placement decisions ordered after removals over
the same staged world view. General entity scheduling and advisory commit
observations are implemented below; durable notifications remain open.
Registered semantic use actions can now compose ordinary item, entity, drop and
world operations through that plan. Existing built-in recipes and specialized
entity requests have not yet been migrated, and the client discovery surface
does not yet expose arbitrary author-defined action argument schemas.
General entity due handlers now run on the persisted entity clock, with owned
state/next due time and shared world/drop effects in one durable transaction.
This does not yet replace general owner-local world systems or built-in fire.
Read-only committed observers now see public projections off the coordinator.
Their advisory delivery is bounded, lossy and not replayed; authoritative
follow-up must use durable decisions/scheduling. Fire and owner-wave commits
are not yet projected through this observation lane.
Shared gameplay edits now dispatch bounded neighbor/support decisions, including
semantic use and scheduled entity effects. Built-in plant support loss runs via
the public fallback, including its harvest, in the triggering transaction.
Fire burns now use the same removal/support transaction, including anchored
footprint refunds and cross-chunk support effects in the fire WAL receipt. The
fire frontier/propagation and cross-chunk delivery policy remain native-only;
owner-local world-system, drop and player parity remain open.

### Integration follow-up

Post-integration gameplay follow-ups are tracked in the root plan's **Completed
task: player-response path hardening** section. `ecb07ee` fixes reproduced entity
reservation starvation; `36e177b` prioritizes direct edit meshes and separates
mobile own-state use from movement-frame CAS (anchored/inventory fences remain
strict). These verified local fixes do not certify end-to-end responsiveness.
The approved fairness/latency hardening pass is now implemented and personally
reviewed without subagents: 702 tests, strict Clippy/fmt, release mixed-load and
restart measurements, and inspected rendering preview. The original tactical
drain/three mesh lanes are replaced by fixed bounded admission turns and fair
two-lane worker/upload policy. See `docs/PLAYER-RESPONSE-PATH.md`; live window
presentation latency remains observable through opt-in tracing, not a claim made
from headless frame benchmarks.

The baseline findings below are retained as the audit record. The integrating
branch has since addressed these items; final combined verification is tracked
in the root plan:

- **F2:** exact automation slot selection and snapshot-local component-equivalence
  keys are implemented. Component predicates/output policies and numeric-remap
  recovery are covered by `server/durable/actions/machine_component_tests.rs`.
- **Anchored lifecycle / F4:** registered own-state initialization, codecs,
  projections, interaction and support/neighbor reactions are implemented. Fire
  footprint invalidation now combines terrain/entity/refund changes with the fire
  state in one WAL record. This is lifecycle compatibility, not migration of fire
  behavior to the public API. See `docs/ANCHORED-BEHAVIORS.md`.
- **Recovery review fix:** anchored output encoding now checks canonical decoding
  before admission. `anchored_codec_tests.rs` rejects an unrecoverable callback
  output before WAL admission and checks valid-state recovery.
- **F3:** block planners now capture explicit read-only terrain stamps, including
  support cells and absence checks. Admission reserves those chunk keys until
  apply; stale plans are rejected after reservations clear. The seam race test is
  `server/durable/actions/support_reads_tests.rs`.
- **F7:** public persistent owner systems and an independent region-clock fixture
  now exist, with real-listener restart verification. The supported API is still
  owner-only; terrain reads/effects and full world-system parity remain open.
  See `docs/REGISTERED-SYSTEMS.md`.

Content/composition and stock-client action discovery are now merged and personally
reviewed in main. Public state/item/PNG/material/geometry declarations, component
schemas, tags and dependency/capability bundles reach the frozen catalog. Builtin
HUD art now uses public `item_icon` registration rather than item-key dispatch;
Etched Chip proves the independent path, including remap and fingerprint checks.
Registered action discovery reaches bounded stock-client panels and durable
identity/inventory-revision-fenced dispatch. SignalPost targeting includes a
non-anchor footprint cell. Knapping verifies duplicate/stale/restart behavior over
the real listener. Integration additionally validates recipe/fuel constants against
component schemas and prevents incompatible component-preserving output from
consuming input/fuel. Combined tests and previews are recorded in the root plan.

These close the content, composition, inventory-selector, anchored lifecycle and
bounded action/UI findings in that integration. Subsequent shared gameplay work
above covers general harvest and semantic use. World-system reads/effects,
generation, player policy/commands/bindings and world-drop policies remain
blockers; fire migration is approved and still unfinished, not deferred.

Audited baseline: **`0d3eebf616eb7bf2c5a942cc6e199b210168666e`**, on
`modding/audit`. This is a source/call-path audit, not a new runtime test report.
Rust implementation paths below omit the common `src/` prefix (for example,
`server/startup.rs` means `src/server/startup.rs`); `crates/`, `extensions/`, and
`docs/` paths are repository-root relative. Symbols identify the relevant
implementations without depending on line numbers changing during integration.

**Full built-in/mod parity is not complete.** The baseline has usable public
storage, ground-creature, inventory-screen, and inventory-machine slices.
It still has privileged content, interaction, world, player, and drop paths.
The document completes the baseline inventory of gameplay categories; it does
not certify that all findings have been fixed.

Concurrent content/registration, anchored lifecycle, inventory/process,
action/UI/item, and public owner/system work is **not present in this baseline**.
Every affected finding needs an integration review against the merged code.
The root plan is maintained separately. In particular, completing its currently
“In progress” rows must not imply completion of planned world generation,
player rules/commands/bindings, or world-drop parity. **Fire migration is now
authorized but remains a full-parity blocker** until implemented.

Proof labels used below:

- **Public + fixture:** source trace reaches a public host contract and an
  independently compiled fixture plus production-path test exists. Existing
  tests are identified, not claimed freshly run.
- **Public slice:** externally accessible contract and built-in consumer found;
  broader category or particular behavior is not proved by that fact.
- **Internal / blocked:** the live path uses inaccessible types, a private
  registration seam, or privileged policy/dispatch.
- **Integration review:** a concurrent workstream is expected to address this;
  no completion credit is assigned from intent alone.

## Audit boundary and live path

The external boundary is `crates/host-api/src/lib.rs`: `Extension::register`
receives `&mut dyn Registrar`. At baseline its registrations are `CubeBlock`,
`StorageBlockEntity`, `InventoryScreen`, `entity::MobileEntity`, and
`machine::Machine`. A `pub` trait in `src/server/` is not sufficient: this is a
binary implementation with private host types, not an independently usable API.

The development installation path is
`server/startup.rs::ServerStartup::with_extension` →
`server/lifecycle.rs::Registration::install` → candidate `Catalog` validation →
world catalog resolution → frozen server/client adapters. `ServerStartup`'s
private trusted-native entity/system hooks do not enlarge `Registrar`'s surface.
Explicit Rust installation is already supported; dynamic loading is a separate
milestone, not a missing implementation of an existing gameplay rule.

On the authoritative path, `server/durable/actions/mod.rs::plan_durable_request`
plans commands and ticks, `Durability::try_stage` admits exact journal
participants, and receipted publication exposes changes. Entity persistence
uses `server/entities/persistence.rs`, entity checkpoints, and journal recovery;
client assembly uses `client/entities.rs::Replicas::accept` and the protocol's
snapshot/commit groups. A new registration must survive this entire path,
including manifest remapping, rather than work only in a direct planner test.

## Capability matrix

### Definitions, assets, and composition

| Existing category / built-ins | Registration → server behavior | Client and persistence path | Baseline proof and actionable gap |
| --- | --- | --- | --- |
| Opaque terrain cubes: grass, dirt, stone, sand, snow, moss, gravel | `content/builtins.rs::Catalog::builtins` creates `BlockDef`/states/items. `plan_block_edit` validates reach, replaceability, player overlap and inventory debit; collision consumes catalog flags. | `render/mesh.rs::mesh_chunk_lit_with_catalog`, `render/material.rs`, `physics.rs`, `server/voxel_view.rs`; chunk palettes/edits and `content.map` retain state identities. | **Public slice:** `Registrar::cube_block` reaches `content/extensions.rs::extension_cube`, but fixes solidity, opacity, reflectance, swatch, and all gameplay flags. Content integration must expose the existing definition fields and migrate the built-ins onto that same contract. |
| Oriented/stateful blocks: wood axis; kiln facing, half and lit state | `Catalog::register_state_with_emission`, `state_with_property`; builtin state construction is private. Machine variants drive kiln state changes through `entities/machine.rs::Adapter` and durable entity planning. | `StateDef::face_texture`/resolved face layers; kiln lit emission reaches lighting; state keys/properties participate in manifests. `client.rs::edit_for_hit_with_catalog` derives a cardinal `facing` hint. | **Internal / blocked** for external legal state definitions. Expose property domains, legal combinations, state textures/emission and placement-state selection. Check client hints against server validation: ordinary placement accepts only the item's exact `placeable` state, whereas machine variants have a dedicated path. Mere ability to register a nondefault state is not proof players can place it. |
| Cutout leaves and crossed-plane plants: flowers, fern, tall grass | Builtin flags select `CUTOUT`, `PLANT`, `REPLACEABLE`, `SUPPORTS_PLANT`, and flammability. Placement/support policy is in `plan_block_edit`. | `render/mesh.rs` separates cutout cube faces from `emit_plant`; `raycast.rs::plant_intersection` targets plant planes; physics consumes solid flags. | **Internal / blocked** at registration. Expose the geometry choices already implemented, selection behavior, and independent collision/material flags. These paths do not demonstrate arbitrary meshes or arbitrary collision shapes. |
| Glowstone, lit kiln, sky/voxel/bounced lighting | Emission and reflectance come from catalog lookups; lighting workers and `lighting/skylight.rs` implement propagation. | `lighting.rs`, `client/workers.rs`, and render mesh revisions carry light results; material data is fingerprinted. | **Internal / blocked** for assigning external emission/reflectance at baseline, though consumers are mostly data-driven. Content integration needs state-dependent lighting proof, edit/seam relighting and sealed-cave checks. A renderer extension API is not required to expose these existing properties. |
| Texture/material assets: 27 embedded PNG definitions, face texture choice, stitching, alpha cutout | `Catalog::builtins` installs `TextureDef` directly; `Catalog::register_texture` exists only inside the host. `CubeBlock.texture` can reference an existing texture only. | `render/material.rs` builds host-owned materials from catalog textures; fingerprints include texture content/metadata. | **Internal / blocked.** Expose bounded PNG declarations and existing sampling/cutout metadata; preserve startup GPU ownership. Verify registration failure for invalid assets and handshake rejection for differing required assets. |
| Independent items: seeds, sapling, stick; cube and foliage pickups | `content/builtins.rs` installs `ItemDef` (`placeable`, `sprite`, texture/name/swatch). `items.rs::placeable_block_in` is catalog-driven. | Inventory UI and `render/drops.rs` consume item definitions. `inventory.rs::Stack`/`ComponentPayload` and `inventory/store.rs` encode exact stack data. | **Internal / blocked** for registering standalone items/components. Public cubes automatically create only a simple block item. Seeds/saplings are loot/fuel items here, not evidence of an implemented planting/growth action. |
| Namespaced identities, schemas, dependency and ownership composition | `Registration::install` clones the catalog and collects one extension's declarations; `Registry::resolve` checks storage references; machine/mobile/screen binding validates their references. | `content/manifest.rs::resolve_world_catalog` reserves removed IDs and assigns new keys; `resolve_catalog` remaps definitions for the client. `Catalog::identities`/`fingerprint` include supported schemas/presentation. | **Public slice**, **integration review** for dependencies/capabilities/composition. Baseline `Extension` has no dependency/capability metadata. Successive installs are order-sensitive for cross-extension references. Validate duplicate ownership across registration categories, cycles/missing dependencies, and deterministic resolution before opening saves. Internal `SystemRegistry` dependency ordering is not an extension-package composition contract. |

### Blocks, inventories, and processing

| Existing category / built-ins | Registration → server behavior | Client and persistence path | Baseline proof and actionable gap |
| --- | --- | --- | --- |
| Ordinary place/break, replacement and harvest | `plan_durable_request(Edit)` → `plan_block_edit`; registered block hooks are selected by catalog block type. Generic placement consumes one selected item; break/replacement calls `push_harvest_spawns`. | `client.rs::edit_aimed_block`/`edit_for_hit_with_catalog` send edits; journal commits combine world edits, inventory, and drop entities. | **Public slice** for a plain cube; **integration review** for general hooks. `block_actions.rs::BlockEditHook` requires private `BlockActionContext`, `BlockCommitBuilder`, `BlockEditCommand`, and `CommitAction`; it is not a public callback API. |
| Passive chest and multi-cell storage | `entities/chest.rs::Chest` implements public `Extension`; `ServerStartup::block_actions_for` installs `storage_lifecycle::plan_place/plan_break`; `workstation::place/remove` assemble the atomic transaction. `entities/container.rs` provides host-owned inventory handling. | `InventoryScreen` → `client/kiln.rs` generic screen → `ui/draw/container.rs`; versioned `WorkstationView`; container codec and entity journal/checkpoints preserve slots. | **Public + fixture:** external TallStore, nine slots/two cells. Existing `net/tests/extension_lifecycle.rs::external_storage_screen_transfers_reopens_after_restart_and_breaks_over_real_listener` covers real listener, component-bearing transfer, reopening, restart and break. Broader lifecycle hooks are not proved by this slice. |
| Kiln placement, processing, lit footprint, refunds | `content/machines.rs::builtin_machines` declares public `Machine`, variants, recipes/fuels and `Processor`; `ServerStartup::entity_types_for` installs `entities/machine::register`; tick planning runs `machine/planning.rs::Adapter::process`; shared workstation helpers place/remove. | Generic inventory screen supplies fuel/progress fields; kiln lit blocks drive ordinary lighting. `MachinePayload` codec persists slots, fuel/progress, private bytes, variant and schedule. | **Public + fixture** for the machine slice: external Crusher through the real listener. Old `entities/kiln/` implementation is a test reference, not the production parity target. Baseline private bytes start empty; projections, placement costs/refunds and general lifecycle events are not arbitrary. |
| Hopper automation, sided/named machine ports | Public `DownwardFlow` declares push-below/pull-above; `Adapter::transfer` discovers peers from captured views; `durable/actions/entity.rs` validates both ends and batches their changes. | The same inventory screen and machine codec are used. No client animation controls the transfer. | **Public + fixture** for basic transfers. `net/tests/extension_lifecycle.rs::external_processor_manual_and_hopper_transfers_process_restart_and_refund_over_tcp` exercises Crusher with builtin Hopper. **Integration review:** offer selection is reduced to `item`/`count`; `withdraw` chooses the first matching item, losing slot/component identity. See F2. |
| Finite player/container inventories and components | Host `Stack`, `put`/`take`, inventory insertion/transfer, screen slot permissions and machine filters enforce bounded slots and 128-item stacks. Recipes explicitly authorize consumption/production. | `inventory/store.rs` is profile-keyed; `inventory/container.rs` is independent of backpack size. Workstation snapshots contain slots/status; private machine bytes are separate. | **Public slice**, **integration review** for exact selectors and component-aware recipes. Public machine `Slot` exposes only `has_components`; `Recipe`/`Fuel` contain item keys; `Adapter::process` rejects component-bearing inputs/fuel and constructs componentless output. General trusted `EntityTransferPolicy` accepts opaque after-payloads and cannot itself prove conservation; retain host-owned slot operations for new public surfaces. |
| General anchored initialization, projections, use/neighbor/support/invalidation | Baseline storage declarations fix passive inventory lifecycle; machine behavior controls bounded bytes/schedule and a restricted work enum. Hooks own placement/removal only. `ensure_no_unhandled_anchor` refuses generic edits that would orphan an entity. | Storage/machine public projection is `WorkstationView`; a screen is required by binding. Recovery restores entities/footprints; unload is not removal. | **Internal / blocked**, **integration review** for lifecycle work. Expose initialization/codecs/projections and declared costs/refunds without exposing mutable inventories. Cover every footprint cell and every edit producer, including fire/system effects; do not interpret refusal as a supported invalidation callback. |
| Loot: default self-drop, tall-grass seeds, leaves/sticks/saplings | `server/loot.rs::harvest_with_catalog` switches on `AIR`, `TALL_GRASS`, `LEAVES`; default uses `primary_block_item`. Deterministic roll depends on seed, position and edit version. | `push_harvest_spawns` puts loot entities in the same commit as the edit; client later sees authoritative drops. | **Internal / blocked**, **integration review** for item/harvest work. Register equivalent deterministic bounded rules and route displaced plants, support-loss plants, normal harvest and anchored removal consistently. A public drop spawn alone does not remove the privileged harvest switch. |
| General item use and action effects | Baseline client right-click handles mobile interaction, registered inventory opening, or block placement. No public item-use hook exists. Non-placeable seeds/saplings/sticks have no such action. | Fixed `ClientMessage` variants and receipt encoding; empty-space/item-target action discovery is absent. | **Internal / blocked**, external item-action proof is **integration review**. Expose the existing use/placement/harvest mechanisms plus the planned proof action; do not claim a pre-existing crop, tool durability, combat or crafting feature needs migration. |

### Entities, player behavior, world simulation and generation

| Existing category / built-ins | Registration → server behavior | Client and persistence path | Baseline proof and actionable gap |
| --- | --- | --- | --- |
| Ground creature: Mossbun idle/wander/pause, sensing, gravity/navigation, terrain wake | `content/creatures.rs`/`creatures/mossbun.rs` declare `MobileEntity`; `entities/mobile.rs::Adapter` calls public `Behavior` with captured `World` services. Host validates returned motion against locomotion-service results; `mobile_lifecycle.rs` validates bounded spawn/self-removal. | Registered cuboids/animation → `client/entities/registry.rs::project`, `client/entities/targeting.rs`, `render/avatars/mesh.rs`. Bounded private/public codecs and due times use generic entity persistence. | **Public + fixture:** separately compiled Copperling in `extensions/lifecycle-fixture/src/creature.rs`, real listener `net/tests/extension_creature.rs`; adjacent mobile/navigation/locomotion tests cover constraints. This proves ground creatures, not players, drop policy, general inventories, or arbitrary render callbacks. |
| Creature interaction and spawning | Public `MobileEntity.interaction` is one default bounded request; `Behavior::interact` replaces only own state. `durable/actions/entity.rs::plan_interact` checks identity/revision/reach and mobile occlusion. Admin spawning resolves registered mobile definitions. | `client.rs::interact_aimed_mobile`; public pose controls presentation only. Spawn/update/despawn are journaled. | **Public slice.** General multi-action discovery, actor-aware interaction effects and UI are **integration review**. A single own-state request should not be described as arbitrary gameplay interaction. |
| Player input, movement, collision and joining/spawn | `builtins.rs` installs trusted `adapters::player_movement`; `movement/coordinator.rs::advance_players` → `movement.rs::process_movement_batch` → `voxel_view::resolve_player_movement`. `spawn.rs::spawn_position[_cached]` chooses a safe surface near origin. `entities/player.rs` registers a special player type. | `client/movement.rs` prediction/reconciliation and fixed input keys; player adapter/`AvatarModel` presentation. `position_store.rs`, profile inventory and session-bound player entity ownership persist/reconcile different parts of player state. | **Internal / blocked — planned blocker.** Public `MobileEntity` cannot reproduce player policy/session ownership, spawn policy, movement allowance or avatar appearance. Expose supported rule/appearance hooks while leaving authentication, authoritative collision, sequence accounting and profile ownership with the host. |
| Commands/bindings and admin grant/spawn | `client/admin.rs::parse` has closed Help/Give/Spawn variants; `durable/actions/admin.rs::plan_grant/plan_spawn` checks admin identity and plans changes. `client/events.rs::window_event` hard-codes gameplay bindings. | Admin UI and fixed protocol messages; grants/spawns have durable action receipts. Item/creature choices are catalog-driven, but command definitions are not. | **Internal / blocked — planned blocker.** Expose command/action descriptors and bindings for the existing operations; keep permission checks server-side. Generic UI/action work alone does not prove command or movement-binding parity. |
| Plant support and terrain-dependent wake behavior | `plan_block_edit` checks `PLANT` support, removes a plant above broken supporting soil, and harvests displaced plants. Mobile/drop planners use persisted schedules and terrain dependency wake paths. | Chunk edits/light/mesh updates are replicated; entity schedules/dependencies survive via host machinery. | **Internal / blocked** for general block support callbacks; **integration review** for owner/system and lifecycle work. Baseline support cleanup is inline edit policy, not a registered world system. Support checks must also participate in read dependencies (F3), and run for non-player edit producers. |
| Growth/world systems | `SystemRegistry`, `SystemHandler`, owner codecs, effects and `runtime/systems.rs::SystemRuntime` exist internally. `ServerStartup::register_system/register_owner_codec/seed_owner` require private host types. Baseline registered owner waves reject neighbor snapshots. | Owner values, wake flags and rotation cursors use the main WAL and recovery; baseline public Registrar has no system method. | **Internal / blocked**, **integration review** for main-owned public owner/system fixture. No live timed sapling/crop growth consumer was found: builtin phase registration contains input, durable actions, movement, fire, interaction and publish. Terrain decoration is generation, not timed growth. Prove public bounded world reads/effects separately from owner-counter persistence; do not invent a builtin growth migration to certify parity. |
| Fire propagation/delivery and cross-chunk intent | `builtins.rs::register_builtin_systems` gives `FireHandler`/`FireDeliveryHandler` trusted drivers. `fire/scheduler.rs` prepares source/delivery waves; `durable/fire.rs::stage_wave` admits fire transactions; `apply_synced_batch` directly installs prepared world edits. | Fire frontier/pending records have dedicated codecs/checkpoint/recovery. Committed block edits feed normal streaming and terrain-dependent wake handling. | **Internal / blocked — deferred blocker.** Public owner registration does not migrate these trusted paths. Preserve source/delivery ordering, durable pending work and bounded cross-chunk reads when authorized. Lifecycle compatibility must be reviewed even while migration stays deferred (F4). |
| Terrain, biomes, caves/surface patterns, trees and foliage | `world/terrain.rs::generate_chunk` → `terrain_column`, `generated_block_with_pattern`, `decorate_chunk`; `tree_anchor/tree_piece/ground_plant` choose builtin IDs. `generated_block` supplies equivalent per-cell procedural fallback. | Server chunk loading generates base chunks then overlays saved edits; clients receive authoritative chunks. Procedural fallback is not edit authority. `storage.rs` and world chunk caches retain edits over generated terrain. | **Internal / blocked — planned blocker.** No deterministic generation-contributor registration, bounded output/ordering or seam-ownership API. Expose both chunk and per-cell semantics; a modded generation path cannot leave fallback queries using incompatible builtin terrain. Test trees across chunk seams and load order; persist/check generator identity without adding prerelease converters. |
| World drops: gravity/rest/support, merging, pickup delay/radius, expiration, inventory pickup | `drops/entity.rs::register_entity_type` explicitly registers type 1 and `DropTickPlanner`; `drops/planning.rs::plan_stack_spawns`/`merge_target` implement bounded component-preserving merges in stable ID order; `plan_take`/`plan_expired` remove stacks. `drops/queries.rs` selects pickups/expiration. `plan_durable_request(Pickup/Expire/DropStack)` owns credit/debit/expiry and fixed throw delay. | `DropEntityPayload` stores exact stack, creation time, pickup delay and fall speed; generic entity/WAL persistence carries position/schedule. Separate `DroppedItem` snapshots and explicit pickup events feed `client/drops.rs::DropAnimator` → `render/drops.rs`. | **Internal / blocked — planned blocker.** Public cube items can appear as drops but cannot register equivalent drop gameplay/presentation policy. Public mobile creatures do not expose pickup ownership, merging, delay/expiry, item projection or pickup flight. Preserve component stacks and receipt-driven pickup; animation must remain presentation-only. |

### Interaction, presentation and durability closure

| Existing category | Concrete path | Baseline proof and actionable gap |
| --- | --- | --- |
| Inventory screens, layouts, status and tooltips | `InventoryScreen`, `SlotGroup`, `StatusField` → `content/inventories.rs` → `protocol/workstation.rs::WorkstationView` → `client/kiln.rs`, `ui/layout.rs`, `ui/draw/container.rs` | **Public + fixture** for 1–54 slots and bounded status fields. `ui/types.rs` has closed screens/controls; no public arbitrary screen composition/action control surface. Screen registration is not server authorization: slot roles, target identity, revision and filters are checked again by entity policies. General action/UI integration must cover actual discovery, opening, control dispatch and close/reopen behavior. |
| Legacy kiln quick actions | `client/events.rs` R/F bindings → `client.rs::interact_aimed_entity` → `client/entities/kiln.rs::kiln_adapter` → fixed version-1 request → `entities/machine/inventory.rs::Adapter::plan` | **Privileged production path**, despite generic machine runtime. It is not just dead legacy code. Replace with registered action discovery/dispatch and a uniform identity/revision fence; see F1. |
| Player vs registered creature rendering | `client/entities/registry.rs::builtins` installs player/kiln adapters; `project` separately handles registered mobiles/screens. `render/avatars.rs`/`avatars/mesh.rs` include player-specific presentation. | Creature cuboids/gait are public. Player appearance and drop pop/spin/flight are distinct remaining capabilities. Unknown internal entity adapters currently fall through to stored-but-undrawn; required custom projection support must fail startup/handshake rather than silently disappear. |
| Save/wire identities and required definitions | `Catalog::identities/fingerprint`, `ContentManifest::resolve_world_catalog/resolve_catalog`, `storage.rs`, `protocol.rs`; machine/mobile/screen metadata feeds catalog compatibility | **Public slice** for existing declarations. All new schemas, action requests, projections, component contracts and required assets must join identity/compatibility validation and remapping. Copying definition tables only into the startup catalog is insufficient if `resolve_catalog` drops them. Behavior code is not hashed automatically: declared schema fingerprints remain the author's semantic-version responsibility. |
| Atomicity, recovery, retry and publication | `durable/admission.rs`, `durable/coordinator.rs`, journal writer/recovery, entity recovery/checkpoints, `durable/publication/`, `client/entities.rs::Replicas` | Shared host mechanisms are accessible through the existing adapters. Every new public effect must use them: read dependencies, bounded pre-admission validation, exact inventory operations, one receipt and post-receipt publication. An external compile test or a callback that only mutates its private counter does not prove world/entity/inventory atomicity. |
| Runtime/package distribution/hot reload | Public Rust fixture installation only | **Separate deferred product milestone.** No inference about sandbox isolation, native ABI stability, dependency download, dynamic loading or hot reload follows from this audit. |

## Cross-cutting integration findings

### F1 — Registered actions must replace the still-live kiln shortcut

`client/events.rs` binds R/F to constants from `client/entities/kiln.rs`.
`EntityClientRegistry::builtins` installs `kiln_adapter` explicitly. The machine
adapter accepts both six-byte version-1 requests and version-2 requests; only
version 2 includes entity ID and revision. The generic target is resolved from
the cell before the policy runs. Thus the old shortcut has different freshness
semantics from the generic screen and can address a replacement at the same cell.
This is an observed API/validation difference, not a demonstrated exploit.

**Followup (action/UI + inventory):** migrate quick actions onto registered
descriptors and use the same target/schema/revision authorization as screen
controls. Verify stale request after replacement, out-of-reach/occluded target,
disallowed slot, duplicate receipt, and external quick action. Do not preserve an
unfenced compatibility branch solely for this prerelease protocol.

### F2 — An automation offer is not the stack that will be withdrawn

`entities/machine/planning.rs::Adapter::transfer` examines full offered stacks
for destination acceptance but constructs `EntityItemTransfer { item, count,
route, ... }`. `entities/machine/inventory.rs::withdraw` then selects the first
eligible slot matching only that item. Two same-item stacks with different
components can cause the accepted later offer to resolve to the earlier stack.
The final deposit can refuse it, so this is **not evidence of item creation**;
it is an exact-selection/API gap and a potential persistent automation stall.

**Followup (inventory/process):** carry a stable slot/stack selector and source
revision from discovery through authoritative withdrawal, including container
and machine peers; validate destination port/face and exact component equality
at commit. Prove two same-item component variants, a stale source selector,
partial-stack transfer and restart. Recipe progress identity must distinguish
the selected recipe/component predicate, not just `progress_item: ItemId`.

### F3 — Read-only support cells need journal dependency protection

`plan_block_edit` reads soil below a plant (and the cell above removed soil)
through `cached_block_or_request`, but only edited coordinates enter
`World::prepare_edits`. `Durability::try_stage` derives read keys from an optional
entity transaction and changed-cell anchored occupancy; the generic branch
does not explicitly retain these extra terrain reads. At a vertical chunk seam,
a read-only support chunk differs from the edited chunk, so write-key conflict
protection alone is insufficient evidence of serializable support policy.

**Followup (lifecycle + owner/system):** capture all authoritative reads, including
negative/absence checks, with revision/dependency validation and reservations.
Exercise a plant placement racing support removal across a chunk boundary while
a WAL receipt is pending. This audit identifies the missing explicit dependency
path; it does not claim a newly reproduced failing runtime test. General callback
read APIs should solve this once rather than duplicate ad hoc cached reads.

### F4 — New flammability meets a separate edit producer

Ordinary edits use `BlockActionRegistry`/`ensure_no_unhandled_anchor`; registered
machine ticks use footprint validation. Fire uses `FireTransaction` and
`durable/fire.rs::apply_synced_batch`, installing world edits through
`FireRuntime::apply_synced_world_edits`. It does not dispatch ordinary block
placement/removal hooks. Baseline storage/machine content is nonflammable, which
limits this intersection today; richer content registration makes it reachable.

**Followup (content + lifecycle + main):** audit fire and every new system/action
world-edit effect for anchored footprint invalidation, support cleanup, exact
refunds and dependency fencing. Use a flammable multi-cell storage case with
contents and a neighboring supported plant. Keep fire migration deferred until
authorized, but report any unsupported lifecycle combination honestly; do not
mark general all-edit-path lifecycle parity complete while it remains uncovered.

### F5 — New registrations must survive both catalog reconstruction and composition

`ContentManifest::resolve_catalog` constructs a fresh catalog and explicitly
copies/rebinds supported declarations. New registration tables need equivalent
copy/remap and fingerprint coverage. `Registration::install` resolves each
extension immediately; forward references between separately installed
extensions are order-dependent at baseline. Lifecycle ownership is checked in
several places, including `ServerStartup::block_actions_for`, so validation at
only one builder is not the complete conflict contract.

**Followup (content/registration + all adapters):** install two independently
compiled extensions in both orders, resolve the saved world assignments, connect
a compatible client, and reject changed schema/asset/required capability. Test
cross-category duplicate ownership and unsupported client projection explicitly.
Reserve builtin numeric IDs and removed IDs; incompatible prerelease semantics
should select a new world-folder version rather than introduce converters.

### F6 — Builtin kiln fuel discovery is frozen before external content arrives

`content/machines.rs::builtin_machines` builds a fuel allowlist from the catalog
as it exists during `Catalog::builtins`: special stick/sapling values, wood's
special value, and other flammable placeables. `with_extension` installs content
after that. Registering a flammable external block therefore does not establish
that the builtin kiln will accept it. Creating a new machine with its own recipe
is already possible; extending another registered machine's inputs is a different
composition capability.

**Followup (registration + inventory):** specify whether fuels/recipes are owned
lists or ordered/tag-based contributions; resolve them after all declarations,
with deterministic conflict handling and fingerprints. Prove an external fuel
through the builtin kiln if that interoperability is claimed. Do not confuse
namespaced builtin data in a declaration with a runtime privileged switch: the
gap here is timing/composition, not the mere spelling of builtin item names.

### F7 — Public owner persistence alone does not establish world-system parity

The baseline generic owner runtime schedules and journals owner values/wakes,
but refuses nonzero neighbor radius and has no public terrain read/effect API.
Trusted fire has a separate captured-world driver. A separately compiled owner
counter can prove registration, codecs, schedules and recovery without proving
bounded world reads, world edits, item/entity effects or support migration.

**Followup (main):** record exactly which public reads/effects are supported and
how unavailable/budget-exhausted/stale outcomes differ. Prove at least one actual
external world contribution through real startup, journal, replication and
restart; review its interaction with F3/F4. Do not label owner-only persistence
as completion of the planned world-system API or deferred fire migration.

## Evidence and integration acceptance

Existing external evidence lives in `extensions/lifecycle-fixture/` (separate
crate), `server/durable/actions/extension_tests.rs`,
`server/net/tests/extension_lifecycle.rs`,
`server/net/tests/extension_creature.rs`, and client inventory/mobile tests.
The TCP tests call `reactor::serve_listener_until`, exercising the production
listener path rather than substituting a blocking test server. Existing
`docs/HOST-LIFECYCLE.md`, `REGISTERED-INVENTORIES.md`, `DYNAMIC-ENTITIES.md`, and
`REGISTERED-MACHINES.md` describe these slices and their limits. Historical
test counts in the root plan are not a result of this audit.

Integration review should attach concrete merged paths/symbols and proof results
to each changed matrix row. In particular:

1. **Content/registration:** external stateful/cutout/emissive block, independent
   sprite item/components and texture; builtin migration; remapped manifest and
   incompatible-definition rejection; composed dependency/ownership validation.
2. **Anchored lifecycle:** initialization/projection, costs/refunds, use/support/
   neighbor callbacks; non-anchor footprint removal, unload/recovery, concurrent
   edits and non-player invalidation without duplicate contents.
3. **Inventory/process:** exact selectors, component predicates/results, recipe
   progress identity and full conservation across manual/automatic/retry paths.
4. **Action/UI/item:** external registered action and bounded custom UI control
   actually discoverable by the client; server authorization, versioned requests,
   legacy shortcut migration, deterministic harvest and item-action proof.
5. **Owner/system:** separately compiled public fixture and the exact world
   contribution demonstrated; scheduled state/recovery/wake semantics and complete
   read/effect dependencies. Distinguish owner-only proof from world effects.
6. **Cross-category proof:** run the combined fixture with all registrations,
   builtin peers, real nonblocking listener/client assembly, isolated save and
   restart. Inspect appropriate UI/render previews for new presentation, rather
   than infer visual correctness from compilation. Run implementation checks in
   the integrating branch; this document-only audit did not rerun them.

World generation, player policy/commands/bindings, world-drop behavior and
presentation, and deferred fire each still require their own migration and
external-access proof. They remain blockers even if items 1–6 pass.

## Existing capabilities versus new gameplay

Parity includes today's cubes/state-dependent face art, cutout foliage,
cross-plane plants, emission/reflectance, finite inventories/components,
storage/machines, deterministic loot, ground creatures, authoritative player
movement/spawn/admin commands, support cleanup, fire, terrain/vegetation
generation and world-drop physics/presentation. Host scheduling, collision,
journal, networking, GPU resource management and authentication remain mechanisms
that extensions use through bounded operations, not raw mutable state.

This baseline supplies no implemented crop-growth loop or sapling planting
action to migrate. Likewise, liquids, combat/health, tool durability, audio,
skeletal animation and arbitrary GPU programs are not requirements inferred by
this audit. Future gameplay and any deliberately added proof action should be
identified as new work and use the same public capabilities; they cannot stand
in for migrating the existing privileged paths listed above.
