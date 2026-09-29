--- Editor-only callback types for Bloxgloom package modules. Annotate callback
--- arguments explicitly; these are not globals or a replacement for host validation.
---
--- Exact identities and counters are immutable host-created handles. Pass IDs
--- back unchanged, compare IDs/revisions with ==, and use tick helpers for time.
--- These handles are values, not permission grants; host authority checks apply.

--- Exact entity identity. Host-created and immutable; supports == and tostring.
--- Use as a table key or pass directly to entity/inventory services. No arithmetic.
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
    register_handler: (key: string, revision: number, event: "BlockRemoved" | "BlockPlaced" | "NeighborChanged" | "EntityTick" | "PickupRequested", target: string, module: string) -> (),
    register_entity: (key: string, schema_version: number, state_bytes: number, public_bytes: number, initial_delay_ticks: number?) -> (),
    register_system: (declaration: BloxSystemDeclaration) -> (),
    register_storage: (entity: string, previously_registered_block: string, title: string, slots: number, columns: number) -> (),
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
    --- Target entity identity, or nil when this is not an entity target.
    entity: BloxEntityId?,
}
--- Server transaction services. Proposed mutations commit together; an error rejects the transaction.
export type BloxGameplayContext = {
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
