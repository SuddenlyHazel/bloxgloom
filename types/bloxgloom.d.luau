--- Editor-only callback types for Bloxgloom package modules. Annotate callback
--- arguments explicitly; callback aliases do not create globals or replace host validation.
--- The log declaration describes the runtime global installed by the shared sandbox.
---
--- Exact identities and counters are immutable host-created handles. Pass IDs
--- back unchanged, compare IDs/revisions with ==, and use tick helpers for time.
--- These handles are values, not permission grants; host authority checks apply.

--- Diagnostics describe an execution attempt, not a committed gameplay effect.
--- 64 records / 16 KiB per invocation; messages <=2048 UTF-8 bytes.
--- At most 16 fields: keys <=64 bytes, string values <=1024 bytes; no nested tables.
export type BloxLogFields = {[string]: string | number | boolean}
--- Generic field argument accepts ordinary inline records; runtime validates scalar values.
declare log: {
    trace: <T>(message: string, fields: T?) -> (),
    debug: <T>(message: string, fields: T?) -> (),
    info: <T>(message: string, fields: T?) -> (),
    warn: <T>(message: string, fields: T?) -> (),
    error: <T>(message: string, fields: T?) -> (),
}

--- Exact entity identity. Host-created and immutable; supports == and tostring.
--- Use as a table key or pass directly to entity/inventory services. No arithmetic.
--- Exact connection identity: stable profile plus durable server-issued epoch.
declare extern type BloxSessionId with
end

export type BloxPlayerView = {
    profile: BloxProfileId,
    session: BloxSessionId,
    entity: BloxEntityId?,
    name: string,
    position: {number},
    appearance: {skin:number,shirt:number,pants:number,flags:number},
    online: boolean,
    identity_trust: "claimed_profile",
}

declare extern type BloxEntityId with
end

--- Exact profile owner identity. Host-created and immutable; supports == and tostring.
declare extern type BloxProfileId with
end

--- Captured state version. Compare with ==; this is not an entity identity or a tick.
--- In motion_revision, the token versions movement independently of state.
--- Anchored replicas have an initial motion revision (:is_initial() is true).
declare extern type BloxRevision with
    --- Whether this is the initial zero revision. Anchored replicas have initial motion revisions.
    function is_initial(self): boolean
end

--- Logical server simulation time, stored exactly. Supports == and tostring.
declare extern type BloxTick with
    --- True when this tick precedes other, including values beyond numeric precision.
    function before(self, other: BloxTick): boolean
    --- Exact number of simulation ticks since earlier. Errors for reversed intervals or intervals greater than 2^53.
    function elapsed_since(self, earlier: BloxTick): number
end

--- Startup registration services. Definitions are frozen before gameplay; methods use dot calls.
export type BloxStartupHost = {
    register_texture: (key: string, declared_asset: string, options: BloxTextureOptions?) -> (),
    register_block: (key: string, name: string, package_texture: string, options: BloxBlockOptions?) -> (),
    register_item: (key: string, name: string, texture: string, options: BloxItemOptions?) -> (),
    -- A member is a namespaced definition key or "#namespace:tag".
    -- At most 32 tags per package and 32 members per tag; cycles fail startup.
    register_tag: (key: string, kind: "block" | "item", members: {string}) -> (),
    register_generator: (key: string, revision: number, module: string) -> (),
    -- Optional command facet: empty-target only; action key is its name.
    register_action: (key: string, revision: number, label: string, target_kind: "empty" | "item" | "block" | "entity", target_key: string?, module: string, command: BloxCommand?) -> (),
    -- Readonly advisory post-WAL callback; actions/v1, at most eight/package.
    register_committed_observer: (key: string, revision: number, module: string) -> (),
    register_handler: (key: string, revision: number, event: "BlockRemoved" | "BlockPlaced" | "NeighborChanged" | "EntityTick" | "PickupRequested", target: string, module: string) -> (),
    register_entity: (key: string, schema_version: number, state_bytes: number, public_bytes: number, initial_delay_ticks: number?) -> (),
    register_system: (declaration: BloxSystemDeclaration) -> (),
    -- Requires players/v1; own-package key/module, revision 1..65535, private state cap 1..4096.
    register_player_lifecycle: (key: string, revision: number, max_state_bytes: number, initial_state: string, module: string) -> (),
    register_creature: (declaration: BloxCreatureDeclaration) -> (),
    register_machine: (declaration: BloxMachineDeclaration) -> (),
    register_storage: (entity: string, previously_registered_block: string, title: string, slots: number, columns: number, options: BloxStorageOptions?) -> (),
    -- Requires content/v1. Exactly one selection across the entire package set;
    -- key belongs to this package, revision is an integer in 1..4294967295.
    -- Immutable for this world/session. Any change (including key/revision or
    -- removal) refuses an existing world's content.map; pcall cannot suppress
    -- invalid or duplicate declarations. No declaration selects builtin rules.
    register_player_rules: (key: string, revision: number, fields: BloxPlayerRules) -> (),
    -- One startup owner; up to 24 additional linear RGB colors per list.
    -- Appends to builtin skin/shirt/pants indices (6/8/6). Model must be
    -- "bloxgloom:humanoid/v1". No custom geometry or physics are implied.
    register_player_appearance: (key: string, revision: number, model: string, palettes: BloxPlayerAppearance) -> (),
}

--- Storage footprint offsets are integer triples (-2..2), include {0,0,0}, and have at most eight cells.
export type BloxStorageOptions = {
    footprint: {BloxTriple}?,
    hint: string?,
    --- Ordered groups must cover every slot exactly once.
    groups: {{label: string, count: number}}?,
}

--- Ground creature declaration. Bodies and model cuboids are host validated; no imported models.
export type BloxCreatureDeclaration = {
    key: string, module: string, schema: number, revision: number,
    max_state_bytes: number, initial_state: string?, interval: number,
    read_radius: number?, reads_neighbours: boolean?, wakes_on_terrain_change: boolean?,
    body: {half_width: number, height: number, speed: number},
    interaction: string?,
    animation: {
        stride_rate: number?, stride_amplitude: number?, idle_rate: number?,
        idle_bob: number?, walk_bob: number?, fall_stretch: number?, landing_squash: number?,
    }?,
    model: {{min: BloxTriple, max: BloxTriple, color: BloxTriple,
        motion: ("body" | "left_foot" | "right_foot")?}},
}

--- Exact binary item components. Version is 1..65535; bytes are 1..1024 bytes.
export type BloxComponents = {version: number, bytes: string}
export type BloxMachineRecipe = {
    key: string, input: string, input_count: number,
    input_components: ("empty" | "present" | BloxComponents)?,
    output: string, output_count: number,
    output_components: ("empty" | "preserve_input" | BloxComponents)?,
    pulses: number,
}
--- Automation slots here are 1-based; gameplay inventory methods use 0-based slots.
export type BloxMachinePort = {name: string, faces: {BloxTriple}, insert: {number}?, extract: {number}?}
export type BloxMachineDeclaration = {
    entity: string, block: string, module: string, schema: number, revision: number,
    interval: number, title: string, hint: string?, footprint: {BloxTriple}?,
    --- Declare exactly one of recipe or recipes (up to eight disjoint recipes).
    recipe: BloxMachineRecipe?, recipes: {BloxMachineRecipe}?,
    fuel: {item: string, pulses: number, components: ("empty" | "present" | BloxComponents)?}?,
    active_state: string?, variants: {{state: string, active_state: string?}}?,
    ports: {BloxMachinePort}?,
}

export type BloxBlockOptions = {
    flammable: boolean?,
    supports_plant: boolean?,
    solid: boolean?,
    replaceable: boolean?,
    side: string?,
    bottom: string?,
    emission: number?, -- 0..15
    reflectance: {number}?, -- exactly three channels, 0..255
    geometry: ("cube" | "crossed_plant" | "narrow_crossed_plant")?,
    material: ("opaque" | "cutout")?,
    properties: {[string]: {string}}?, -- 1..8 names, 1..16 values each
    states: {BloxBlockState}?, -- 1..32 explicit legal states
}
export type BloxBlockState = {[string]: string | number | BloxStateFaces} -- property=value; optional emission=0..15 and textures
export type BloxStateFaces = {top: string, side: string, bottom: string}
export type BloxTextureOptions = { alpha_cutout: boolean? }
export type BloxItemComponents = {
    version: number, -- 1..65535
    --- Stable component schema fingerprint as exactly 16 hexadecimal digits; must be nonzero.
    fingerprint: string,
    max_bytes: number, -- 1..1024
    required: boolean?,
}
export type BloxItemOptions = {
    sprite: boolean?,
    drop_size: ("small" | "normal" | "large")?,
    components: BloxItemComponents?,
    -- Existing drop_animation/drop_policy tables are described in REGISTERED-CONTENT.md.
    drop_animation: {[string]: number}?,
    drop_policy: {[string]: number}?,
}

-- All nine fields are required finite numbers; unknown fields are rejected.
-- Geometry/input/eye values compile to f32; budget remains f64. Exact compiled
-- values are delivered to clients before Welcome and fingerprinted in saves.
export type BloxPlayerRules = {
    half_width: number, -- 0.125..0.5
    foot_inset: number, -- 0..0.25
    middle_height: number, -- above foot_inset, gap <= 1
    head_height: number, -- above middle_height, gap <= 1, at most 2
    intent_blocks_per_second: number, -- at least 0.1, at most budget
    budget_blocks_per_second: number, -- at most 16
    headroom: number, -- integer 2..128
    max_rise: number, -- integer 1..1024
    eye_height: number, -- foot_inset..head_height
}

export type BloxPlayerAppearance = {
    skins: {{number}}?,
    shirts: {{number}}?,
    pants: {{number}}?,
}

-- Ordered positional schema, at most 8 fields and 130 encoded bytes in total.
-- Keys: u8 length + namespaced ASCII bytes (3..max_bytes, max_bytes <= 128).
-- Counts: one byte, 1..128. Defaults make trailing text arguments optional;
-- wire/event.arguments always includes every field with explicit default values.
export type BloxCommandArgument = { kind: "item_key" | "entity_key", max_bytes: number }
    | { kind: "count", default: number? }
    | { kind: "player" }
export type BloxCommand = { permission: "Player" | "Admin", arguments: { BloxCommandArgument }? }

--- Chunk coordinates, or a fixed-width hexadecimal owner identity (16 digits for entity, 32 for profile).
export type BloxSystemSeed = { x: number, y: number, z: number, data: string }
    | { id: string, data: string }
export type BloxSystemDeclaration = {
    key: string,
    schema: number,
    revision: number,
    module: string,
    max_state_bytes: number,
    max_jobs_per_tick: number,
    read_world: boolean?,
    read_radius_chunks: number?, -- 0 (default) or 1; requires read_world=true
    accepts_intents: boolean?, -- requires read_world=true
    intent_bootstrap: string?, -- constant initial bytes for absent destinations
    after: {string}?, -- up to 16 existing system keys; startup rejects cycles
    edit_cause: ("world_edit" | "burn")?, -- burn requires read_world=true; only air removals
    partition: ("chunk" | "entity" | "profile")?,
    creates_drops: boolean?,
    creates_entities: boolean?,
    reads_entities: boolean?,
    mutates_entities: boolean?,
    seeds: { BloxSystemSeed },
}

--- Deterministic chunk authoring context. Chunk coordinates identify the chunk; local voxel coordinates address cells within it.
export type BloxGenerationContext = {
    chunk_x: number,
    chunk_y: number,
    chunk_z: number,
    world_position: (lx: number, ly: number, lz: number) -> (number, number, number),
    builtin_terrain_height: (x: number, z: number) -> number,
    builtin_base_block: (x: number, y: number, z: number) -> string,
    --- Deterministic sample in [0, 1). The host consumes the exact world seed internally; optional salt is an integer in 0..4294967295.
    random_at: (x: number, y: number, z: number, salt: number?) -> number,
    set_block: (lx: number, ly: number, lz: number, state_key: string) -> (),
}

export type BloxBlock = {
    state: string,
    block_type: string,
    primary_item: string?,
    plant: boolean,
    supports_plant: boolean,
}
-- Host triples are readonly, 1-indexed Luau sequences at runtime.
--- Readonly, 1-indexed coordinate sequence: [1] = x, [2] = y, [3] = z. Units depend on the field.
export type BloxTriple = { number }
--- Server-authorized action input. Optional target fields depend on the registered action target kind.
export type BloxActionEvent = {
    kind: "ActionRequested",
    --- Namespaced key of the registered action being requested.
    action: string,
    --- World position supplied by the server action dispatcher, as an x/y/z sequence.
    position: BloxTriple,
    --- Target world cell as integer x/y/z coordinates, when this action has a cell target.
    cell: BloxTriple?,
    --- Selected player inventory slot, numbered from 0. Index inventory arrays using slot + 1.
    slot: number,
    --- Raw validated action argument bytes. May contain zero bytes; empty string means no arguments.
    arguments: string,
    --- Readonly ordered values for command actions; player fields are exact session handles.
    command_arguments: { (string | number | BloxSessionId) }?,
    --- Target entity identity, or nil when this is not an entity target.
    entity: BloxEntityId?,
}
--- Removal decisions include deterministic random in [0,1), plus legacy exact halves.
export type BloxRemovedEvent = {
    kind: "BlockRemoved", cell: BloxTriple, previous: BloxBlock,
    cause: "Break" | "Replacement" | "SupportLoss" | "WorldEdit" | "Burn" | "AnchoredBreak",
    random: number, random_lo: number, random_hi: number,
}
export type BloxPlacedEvent = {kind: "BlockPlaced", cell: BloxTriple, previous: BloxBlock, placed: BloxBlock}
export type BloxNeighbourEvent = {
    kind: "NeighborChanged", cell: BloxTriple, changed: BloxTriple, previous: BloxBlock, current: BloxBlock,
}
export type BloxEntityTickEvent = {
    kind: "EntityTick", entity: BloxEntityId, position: BloxTriple, tick: BloxTick,
    entity_lo: number, entity_hi: number, tick_lo: number, tick_hi: number,
}
export type BloxPickupEvent = {
    kind: "PickupRequested", position: BloxTriple,
    drops: {{id: BloxEntityId, count: number, entity_lo: number, entity_hi: number}},
}
--- Match kind before accessing event-specific fields.
export type BloxGameplayEvent = BloxActionEvent | BloxRemovedEvent | BloxPlacedEvent
    | BloxNeighbourEvent | BloxEntityTickEvent | BloxPickupEvent

--- Server transaction services. Proposed mutations commit together; an error rejects the transaction.
export type BloxGameplayContext = {
    player_profile: BloxProfileId?,
    players: () -> {BloxPlayerView},
    player_by_profile: (profile: BloxProfileId) -> BloxPlayerView?,
    player_by_session: (session: BloxSessionId) -> BloxPlayerView?,
    --- Requires the executing package's players/v1 capability. Exact online session; after receipt, no replay.
    message_player: (session: BloxSessionId, text: string) -> (),
    kick_player: (session: BloxSessionId, reason: string) -> (),
    --- Registered integer palette indices, players/v1 authority, exact session. Native cosmetic save precedes replication.
    set_player_appearance: (session: BloxSessionId, skin: number, shirt: number, pants: number) -> (),
    --- Available in gameplay action callbacks; captured phase, with read-your-writes.
    world_time: () -> {elapsed_ms: number, cycle_ms: number},
    --- Integer 0 <= phase < cycle_ms; operator-selected admin profile, atomic with this action.
    admin_set_time: (elapsed_ms: number) -> (),
    admin_give: (item: string, count: number) -> boolean,
    admin_spawn: (entity_key: string) -> (),
    player_position: BloxTriple?,
    tick_lo: number,
    tick_hi: number,
    --- Current logical server tick. Use :before and :elapsed_since for time comparisons.
    tick: BloxTick,
    --- Deterministic sample in [0, 1). Optional sequence is an integer in 0..4294967295; defaults to zero.
    random: (x: number, y: number, z: number, sequence: number?) -> number,
    block: (x: number, y: number, z: number) -> BloxBlock,
    set_block: (x: number, y: number, z: number, state_key: string) -> (),
    --- Stage an exact-count move between 0-indexed player inventory slots. Returns false if the move cannot be made.
    transfer: (from_slot: number, to_slot: number, count: number) -> boolean,
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    spawn_stack: (x: number, y: number, z: number, stack: BloxStack, delay_ms: number) -> (),
    --- Read private state only where host ownership permits. Pass an entity ID handle; nil means unavailable.
    entity_state: (id: BloxEntityId) -> string?,
    --- Read a public entity view by its exact ID handle. Nil means unavailable.
    entity: (id: BloxEntityId) -> BloxEntity?,
    --- Read a bounded set of public entity views around a world position. Radius is in blocks; host visibility and limits apply.
    nearby_entities: (x: number, y: number, z: number, radius: number) -> { BloxEntity },
    --- Look up the exact entity identity anchored at a world cell, or nil when absent.
    anchored_entity_at: (x: number, y: number, z: number) -> BloxEntityId?,
    --- Stage owned entity state bytes using the entity ID handle. Host ownership and schema limits still apply.
    update_entity: (entity: BloxEntityId, state: string) -> boolean,
    --- Stage removal of an entity using its entity ID handle. Host ownership still applies.
    remove_entity: (id: BloxEntityId) -> boolean,
    --- Stage a future entity callback using its entity ID handle. Delay is in logical server ticks.
    schedule_entity: (entity: BloxEntityId, delay_ticks: number?) -> boolean,
    --- Read slots for an authorized inventory owner. The returned array is 1-indexed; slot arguments to inventory methods are 0-indexed.
    inventory: (owner: BloxInventoryOwner) -> { BloxSlot },
    --- Stage insertion of the supplied stack. Returns false when the authorized inventory cannot accept it.
    give: (owner: BloxInventoryOwner, stack: BloxStack) -> boolean,
    --- Stage removal from a 0-indexed slot. Returns the removed stack, or nil when the requested removal cannot be made.
    take: (owner: BloxInventoryOwner, slot: number, count: number) -> BloxStack?,
    --- Stage an exact-count move between authorized inventory owners and 0-indexed slots. Returns false if the move cannot be made.
    transfer_inventory: (from: BloxInventoryOwner, source: number, to: BloxInventoryOwner, destination: number, count: number) -> boolean,
    move_slots: (owner: BloxInventoryOwner, source: number, destination: number, count: number) -> boolean,
    --- Ask the host to credit an eligible drop to the acting player's finite inventory. max_count is 1..128.
    collect_drop: (id: BloxEntityId, max_count: number) -> boolean,
}
--- Public gameplay entity view. The ID handle identifies one entity; data contains only its registered public projection.
export type BloxEntity = {
    --- Exact server-assigned entity identity. Pass directly to entity services; immutable and comparable with ==.
    id: BloxEntityId,
    --- Namespaced registered entity definition key.
    entity_type: string,
    --- Entity position in world block units, as x/y/z.
    position: BloxTriple,
    --- Integer world cell anchoring this entity, or nil for an unanchored entity.
    anchor: BloxTriple?,
    --- Registered public projection bytes. This is not the entity's private state.
    data: string, -- registered public projection, not private state
}
--- The acting player, or an exact entity identity. Host access checks still apply.
export type BloxInventoryOwner = "player" | BloxEntityId
--- An item stack. Count is 1..128; component bytes follow the item's registered schema.
export type BloxStack = { item: string, count: number, components: { version: number, bytes: string }? }
--- An inventory slot. A missing stack means empty; insert/extract describe permitted transfers.
export type BloxSlot = { stack: BloxStack?, insert: boolean, extract: boolean }

--- Durable owner planner input. Owner identity is selected by owner_kind; mutations use captured revisions.
export type BloxOwnerContext = {

    --- Private persisted owner state bytes captured for this plan.
    data: string,
    --- Current logical server tick. Use :before and :elapsed_since for time comparisons.
    tick: BloxTick,
    --- Captured state revision. Compare tokens with ==; do not use as an entity ID.
    revision: BloxRevision,
    --- Readonly durable messages available to this owner plan.
    inbox: { BloxOwnerIntentDelivery },
    --- Read the captured world block state at integer x/y/z coordinates; proposed edits do not change this captured read.
    block: (x: number, y: number, z: number) -> string,
    --- Propose a world-cell edit with its expected captured preimage. The host checks the preimage before commit.
    edit: (x: number, y: number, z: number, before: string, after: string) -> (),
    wake: (system: string, x: number, y: number, z: number) -> (),
    --- Stage binary payload delivery to a chunk owner in this system. Coordinates are destination chunk indices.
    send: (x: number, y: number, z: number, binary_payload: string) -> (),
    entities: {BloxOwnerEntity}?,
    block_info: (x: number, y: number, z: number) -> BloxBlock,
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    --- Stage an owned entity update against its captured revision. A wrong/stale revision rejects the plan.
    update_entity: (id: BloxEntityId, revision: BloxRevision, state: string) -> (),
    remove_entity: (id: BloxEntityId, revision: BloxRevision) -> (),
    wake_entity: (system: string, id: BloxEntityId) -> (),
    wake_profile: (system: string, id: BloxProfileId) -> (),
} & (
    {owner_kind: "chunk", owner: BloxTriple}
    | {owner_kind: "entity", owner: BloxEntityId}
    | {owner_kind: "profile", owner: BloxProfileId}
)

--- Package-owned entity captured by an owner plan; state is private to its authorized package.
export type BloxOwnerEntity = {
    id: BloxEntityId,
    revision: BloxRevision,
    key: string,
    position: BloxTriple,
    state: string,
}
--- Durable message identity: source chunk, source owner revision token, and ordinal. Preserve every field for identity comparisons.
export type BloxOwnerIntentId = {
    --- Source owner's x/y/z chunk coordinates.
    source: BloxTriple,
    --- Captured state revision. Compare tokens with ==; do not use as an entity ID.
    revision: BloxRevision,
    --- Ordinal distinguishing messages emitted by the same source owner revision. Preserve it with the source and revision token.
    ordinal: number,
}
--- A durable owner message produced on an earlier logical server tick. Payload is binary data, not necessarily text.
export type BloxOwnerIntentDelivery = {
    --- Complete durable message identity; source, revision token and ordinal together identify the delivery.
    id: BloxOwnerIntentId,
    --- Logical server tick that produced this durable message.
    produced_tick: BloxTick,
    --- Binary bytes authored by the sending owner. Interpret using your system's own payload format.
    payload: string,
}

--- A declared shader parameter value: number, boolean, or a numeric vector. The resource declaration determines its type and range.
export type BloxVisualParameterValue = number | boolean | {number}

--- Session-local presentation setup. These methods cannot edit authoritative world or inventory state.
export type BloxClientStartupHost = {
    --- Set text on an owned document/node identity, such as "demo:controls/title".
    set_text: (owned_document_node: string, ascii_text: string) -> (),
    --- Set explicit local state for an owned document. This is presentation state, not saved gameplay state.
    set_state: (owned_document: string, ascii_state: string) -> (),
    --- Set a declared parameter on an owned material/effect. Its declared type and bounds are checked by the host.
    set_parameter: (owned_resource: string, parameter_name: string, value: BloxVisualParameterValue) -> (),
    --- Select an owned client module for bounded public replica presentation callbacks in this session.
    set_replica_handler: (own_client_module: string) -> (),
    --- Requires players/v1. One own-package module; multiple packages compose.
    set_player_handler: (own_client_module: string) -> (),
}

--- Callback host: presentation only. Registrations are startup-only.
export type BloxClientPlayerHost = {
    set_text: (owned_document_node: string, text: string) -> (),
    set_state: (owned_document: string, state: string) -> (),
    set_parameter: (owned_resource: string, parameter_name: string, value: BloxVisualParameterValue) -> (),
}
export type BloxClientPlayerEvent = {
    kind: "SessionReady" | "PlayerStateChanged" | "SessionDisconnected",
    profile: BloxProfileId,
    session: BloxSessionId,
    identity_trust: "claimed_profile",
    -- Only this package's explicitly public projections, never private state.
    states: {[string]: {revision: BloxRevision, public: string}},
    reason: string,
}

--- Local UI event input. The handler runs in a fresh sandbox; explicit state carries document-local data between events.
export type BloxUiInput = {
    --- Client event sequence used to match asynchronous presentation results. Not a server tick or entity revision.
    sequence: number,
    --- Namespaced widget event key from the authored UI document.
    event: string,
    --- Widget-provided value, such as edited text. May be empty for a button event.
    value: string,
    --- Explicit document-local state from previous presentation updates. Not persisted world state.
    state: string,
    --- Current document text values keyed by node identity.
    texts: { [string]: string },
}
--- Presentation updates or one gameplay action request. Action targets come from the current selection/aim and are checked by the server.
export type BloxUiCommand =
    { op: "text", node: string, value: string }
    | { op: "visible", node: string, value: boolean }
    | { op: "state", value: string }
    | { op: "parameter", resource: string, name: string, value: BloxVisualParameterValue }
    | { op: "action", key: string, arguments: string? }

-- Replica callbacks receive the same local presentation fields as UI callbacks,
-- plus bounded public entities and membership changes. No private state.
--- Identity of an entity entering/leaving the public replica window.
export type BloxReplicaId = {
    --- Exact server-assigned entity identity. Pass directly to entity services; immutable and comparable with ==.
    id: BloxEntityId,
}
--- Readonly public entity snapshot for presentation. State and motion revisions are separate from entity identity.
export type BloxReplicaEntity = {
    --- Exact server-assigned entity identity. Pass directly to entity services; immutable and comparable with ==.
    id: BloxEntityId,
    --- Namespaced registered entity definition key; use it to select presentation behavior.
    key: string,
    --- Captured state revision. Compare tokens with ==; do not use as an entity ID.
    revision: BloxRevision,
    --- Motion version, separate from state revision. Anchored replicas have an initial motion revision.
    motion_revision: BloxRevision,
    --- Registered public projection as binary bytes. Private entity state is never included.
    public: string,
    --- Server-replicated entity position in world block units, as x/y/z.
    position: BloxTriple,
}
--- A bounded session-local public entity window. Replica callbacks have presentation authority only.
export type BloxReplicaInput = {
    --- Client presentation event sequence. Used to associate worker output with this session/event.
    sequence: number,
    --- Mobile entity window or anchored entity window update.
    event: "replica:entities" | "replica:anchors" | "replica:inventory" | "replica:block" | "replica:world" | "replica:action",
    --- Bounded local observation summary. Entity/anchor events report their window count.
    value: string,
    --- Local presentation state supplied to this callback; not authoritative gameplay state.
    state: string,
    --- Local presentation text map supplied to this callback.
    texts: {[string]: string},
    --- Current bounded public entity window. Do not treat this subset as every entity in the world.
    entities: {BloxReplicaEntity},
    --- Exact IDs entering this callback's tracked window. Use each record's id handle.
    entered: {BloxReplicaId},
    --- Exact IDs leaving this callback's tracked window. Leaving the window does not necessarily mean despawn.
    left: {BloxReplicaId},
}
--- Local presentation updates. Entity commands must reference an entity in the current replica window; they do not change gameplay.
export type BloxReplicaCommand =
    { op: "text", node: string, value: string }
    | { op: "visible", node: string, value: boolean }
    | { op: "state", value: string }
    | { op: "parameter", resource: string, name: string, value: BloxVisualParameterValue }
    | { op: "visual", entity: BloxEntityId, yaw: number, bob: number, squash: number }
    | { op: "tint", entity: BloxEntityId, r: number, g: number, b: number }
    | { op: "ember", entity: BloxEntityId, x: number, y: number, z: number }
    | { op: "spark", entity: BloxEntityId, x: number, y: number, z: number,
        r: number, g: number, b: number, size: number?, lifetime_ms: number? }

--- Creature callback input. Tick services read captured terrain; interaction requests carry binary bytes.
export type BloxCreatureContext = {
    event: "tick",
    data: string,
    id: BloxEntityId,
    tick: BloxTick,
    position: BloxTriple,
    neighbours: {BloxCreatureNeighbour},
    route: (x: number, z: number) -> BloxTriple?,
    solid: (x: number, y: number, z: number) -> boolean,
    clear: (x: number, y: number, z: number) -> boolean,
    grounded: (x: number, y: number, z: number) -> boolean,
    walk_edge: (x1: number, y1: number, z1: number, x2: number, y2: number, z2: number) -> boolean,
} | {
    event: "interact",
    data: string,
    request: string,
}
--- Public neighbour snapshot. No private state or authority to mutate the neighbour.
export type BloxCreatureNeighbour = {
    id: BloxEntityId,
    key: string,
    position: BloxTriple,
    public: string,
}
--- Machine planner input. tick and due are exact logical simulation times.
export type BloxMachineContext = {
    data: string,
    tick: BloxTick,
    due: BloxTick,
    fuel: number,
    progress: number,
    --- Readonly 1-indexed slots; nil means empty. Item is the negotiated numeric catalog ID.
    slots: {BloxMachineSlot?},
}
export type BloxMachineSlot = {item: number, count: number, has_components: boolean}

--- Ordered work proposals; the host resolves exact finite stacks and permitted peers.
export type BloxMachineWork = {kind: "process"} | {
    kind: "transfer", offset: BloxTriple, port: string, peer_port: string?,
    push: boolean, count: number?, source_slot: number?, destination_slot: number?,
    item: string?, same_as_slot: number?,
}
export type BloxMachinePlanner = (BloxMachineContext) -> (string, number, boolean | {BloxMachineWork})

--- Captured inputs. Profile bytes persist; session bytes end with this connection.
export type BloxPlayerLifecycleEvent = {
    kind: "PlayerJoining" | "PlayerJoined" | "PlayerLeaving" | "PlayerLeft" | "PlayerSpawned" | "ProfileTick" | "SessionTick",
    profile: BloxProfileId, player: BloxPlayerView?, transition: BloxTick,
    identity_trust: "claimed_profile", state: string, public_state: string, session_state: string,
}
export type BloxPlayerLifecycleDecision = {
    state: string?, public_state: string?, session_state: string?,
    --- Integer 1..100000 ticks; false suspends; omitted retains the deadline (ticks are one-shot).
    profile_delay: (number | boolean)?, session_delay: (number | boolean)?,
    --- Admission only; authoritative collision/terrain validation precedes installation.
    spawn: BloxTriple?, deny: string?,
}
export type BloxPlayerLifecycleCallback = (BloxGameplayContext, BloxPlayerLifecycleEvent) -> BloxPlayerLifecycleDecision?

--- Readonly advisory post-commit event. Delivery can drop and is never replayed.
export type BloxCommittedEntity = {
    kind: "Spawned" | "Updated" | "Removed",
    entity: BloxEntityId,
    entity_type: string,
    position: {number}?,
    anchor: {number}?,
    data: string?,
}
export type BloxCommittedEvent = {
    kind: "Committed",
    blocks: {{cell: {number}, state: string}},
    entities: {BloxCommittedEntity},
    inventory: {profile: BloxProfileId, revision: BloxRevision}?,
}
