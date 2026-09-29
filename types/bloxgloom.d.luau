--- Editor-only callback types for Bloxgloom package modules. Annotate callback
--- arguments explicitly; these are not globals or a replacement for host validation.
---
--- Fields ending in `_lo` and `_hi` hold the low and high unsigned 32-bit words
--- of one 64-bit value. Each word is an exact integer in 0..4294967295.
--- Luau numbers cannot represent every 64-bit integer, so keep both words and
--- pass them back unchanged. Conceptually the value is hi * 2^32 + lo; do not
--- combine entity IDs into a Luau number or use bit32 shifts to reconstruct them.
--- IDs identify entities; revisions identify versions of state; ticks measure
--- server simulation time. These pairs are not interchangeable.

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
    --- Low 32 bits of the item component schema fingerprint. Exact unsigned word; the combined fingerprint must be nonzero.
    fingerprint_lo: number, -- exact unsigned 32-bit halves; combined fingerprint is nonzero
    --- High 32 bits of the same schema fingerprint. Preserve the pair across compatible releases; it is not an entity ID.
    fingerprint_hi: number,
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

export type BloxSystemSeed = { x: number, y: number, z: number, data: string }
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
    seeds: { BloxSystemSeed },
}

--- Deterministic chunk authoring context. Chunk coordinates identify the chunk; local voxel coordinates address cells within it.
export type BloxGenerationContext = {
    --- Low 32 bits of the world generation seed. Keep paired with seed_hi for deterministic generation.
    seed_lo: number,
    --- High 32 bits of the world generation seed. Keep paired with seed_lo; avoid reconstructing a 64-bit Luau number.
    seed_hi: number,
    chunk_x: number,
    chunk_y: number,
    chunk_z: number,
    world_position: (lx: number, ly: number, lz: number) -> (number, number, number),
    builtin_terrain_height: (x: number, z: number) -> number,
    builtin_base_block: (x: number, y: number, z: number) -> string,
    random_at: (x: number, y: number, z: number, salt_lo: number, salt_hi: number) -> (number, number),
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
    --- Low 32 bits of the targeted entity ID. Present only for an entity target; keep paired with entity_hi.
    entity_lo: number?,
    --- High 32 bits of the targeted entity ID. Present only for an entity target; keep paired with entity_lo.
    entity_hi: number?,
}
--- Server transaction services. Proposed mutations commit together; an error rejects the transaction.
export type BloxGameplayContext = {
    --- Low 32 bits of the current logical server tick. Simulation time, not wall-clock time; keep paired with tick_hi.
    tick_lo: number,
    --- High 32 bits of the current logical server tick. Compare high words first, then low words for chronological ordering.
    tick_hi: number,
    random: (x: number, y: number, z: number, sequence_lo: number, sequence_hi: number) -> (number, number),
    block: (x: number, y: number, z: number) -> BloxBlock,
    set_block: (x: number, y: number, z: number, state_key: string) -> (),
    --- Stage an exact-count move between 0-indexed player inventory slots. Returns false if the move cannot be made.
    transfer: (from_slot: number, to_slot: number, count: number) -> boolean,
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    --- Read private state only where host ownership permits. Pass an unchanged entity ID pair; nil means unavailable.
    entity_state: (id_lo: number, id_hi: number) -> string?,
    --- Read a public entity view by its exact ID pair. Nil means unavailable.
    entity: (id_lo: number, id_hi: number) -> BloxEntity?,
    --- Read a bounded set of public entity views around a world position. Radius is in blocks; host visibility and limits apply.
    nearby_entities: (x: number, y: number, z: number, radius: number) -> { BloxEntity },
    --- Look up the entity anchored at a world cell. Returns (id_lo, id_hi), or nil values when absent.
    anchored_entity_at: (x: number, y: number, z: number) -> (number?, number?),
    --- Stage owned entity state bytes using the exact ID pair. Host ownership and schema limits still apply.
    update_entity: (id_lo: number, id_hi: number, state: string) -> boolean,
    --- Stage removal of an entity using its exact ID pair. Host ownership still applies.
    remove_entity: (id_lo: number, id_hi: number) -> boolean,
    --- Stage a future entity callback using its exact ID pair. Delay is in logical server ticks.
    schedule_entity: (id_lo: number, id_hi: number, delay_ticks: number?) -> boolean,
    --- Read slots for an authorized inventory owner. The returned array is 1-indexed; slot arguments to inventory methods are 0-indexed.
    inventory: (owner: BloxInventoryOwner) -> { BloxSlot },
    --- Stage insertion of the supplied stack. Returns false when the authorized inventory cannot accept it.
    give: (owner: BloxInventoryOwner, stack: BloxStack) -> boolean,
    --- Stage removal from a 0-indexed slot. Returns the removed stack, or nil when the requested removal cannot be made.
    take: (owner: BloxInventoryOwner, slot: number, count: number) -> BloxStack?,
    --- Stage an exact-count move between authorized inventory owners and 0-indexed slots. Returns false if the move cannot be made.
    transfer_inventory: (from: BloxInventoryOwner, source: number, to: BloxInventoryOwner, destination: number, count: number) -> boolean,
}
--- Public gameplay entity view. The ID pair identifies one entity; data contains only its registered public projection.
export type BloxEntity = {
    --- Low 32 bits of the server-assigned 64-bit entity ID. Keep paired with id_hi; pass both unchanged to entity services.
    id_lo: number,
    --- High 32 bits of the same entity ID as id_lo. This is not a second ID or a revision; do not combine the pair into a Luau number.
    id_hi: number,
    --- Namespaced registered entity definition key.
    entity_type: string,
    --- Entity position in world block units, as x/y/z.
    position: BloxTriple,
    --- Integer world cell anchoring this entity, or nil for an unanchored entity.
    anchor: BloxTriple?,
    --- Registered public projection bytes. This is not the entity's private state.
    data: string, -- registered public projection, not private state
}
--- The acting player, or an entity identified by its unchanged entity_lo/entity_hi pair.
export type BloxInventoryOwner = "player" | {
    --- Low 32 bits of the inventory entity ID. Pass both entity_lo and entity_hi unchanged; host access checks still apply.
    entity_lo: number,
    --- High 32 bits of the inventory entity ID. Pass both entity_lo and entity_hi unchanged; host access checks still apply.
    entity_hi: number,
}
--- An item stack. Count is 1..128; component bytes follow the item's registered schema.
export type BloxStack = { item: string, count: number, components: { version: number, bytes: string }? }
--- An inventory slot. A missing stack means empty; insert/extract describe permitted transfers.
export type BloxSlot = { stack: BloxStack?, insert: boolean, extract: boolean }

--- Durable owner planner input. This type describes the chunk-owner fields and services.
export type BloxOwnerContext = {
    --- Chunk owner coordinates, as x/y/z chunk indices rather than world block positions.
    owner: BloxTriple,
    --- Private persisted owner state bytes captured for this plan.
    data: string,
    --- Low 32 bits of the current logical server tick. Simulation time, not wall-clock time; keep paired with tick_hi.
    tick_lo: number,
    --- High 32 bits of the current logical server tick. Compare high words first, then low words for chronological ordering.
    tick_hi: number,
    --- Low 32 bits of the captured state revision. A version of state, not an entity identity or simulation tick.
    revision_lo: number,
    --- High 32 bits of the same captured state revision. Keep paired with revision_lo when comparing versions.
    revision_hi: number,
    --- Readonly durable messages available to this owner plan.
    inbox: { BloxOwnerIntentDelivery },
    --- Read the captured world block state at integer x/y/z coordinates; proposed edits do not change this captured read.
    block: (x: number, y: number, z: number) -> string,
    --- Propose a world-cell edit with its expected captured preimage. The host checks the preimage before commit.
    edit: (x: number, y: number, z: number, before: string, after: string) -> (),
    wake: (system: string, x: number, y: number, z: number) -> (),
    --- Stage binary payload delivery to a chunk owner in this system. Coordinates are destination chunk indices.
    send: (x: number, y: number, z: number, binary_payload: string) -> (),
}
--- Durable message identity: source chunk, source owner revision, and ordinal. Preserve every field for identity comparisons.
export type BloxOwnerIntentId = {
    --- Source owner's x/y/z chunk coordinates.
    source: BloxTriple,
    --- Low 32 bits of the captured state revision. A version of state, not an entity identity or simulation tick.
    revision_lo: number,
    --- High 32 bits of the same captured state revision. Keep paired with revision_lo when comparing versions.
    revision_hi: number,
    --- Ordinal distinguishing messages emitted by the same source owner revision. Preserve it with the source and revision pair.
    ordinal: number,
}
--- A durable owner message produced on an earlier logical server tick. Payload is binary data, not necessarily text.
export type BloxOwnerIntentDelivery = {
    --- Complete durable message identity; source, revision pair and ordinal together identify the delivery.
    id: BloxOwnerIntentId,
    --- Low 32 bits of the logical tick that produced this message. Keep paired with produced_tick_hi.
    produced_tick_lo: number,
    --- High 32 bits of the producing tick. Compare high words first, then low words for chronological ordering.
    produced_tick_hi: number,
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
--- One public entity identity split into exact 32-bit words. Keep both words together.
export type BloxReplicaId = {
    --- Low 32 bits of the server-assigned 64-bit entity ID. Keep paired with id_hi; pass both unchanged to entity services.
    id_lo: number,
    --- High 32 bits of the same entity ID as id_lo. This is not a second ID or a revision; do not combine the pair into a Luau number.
    id_hi: number,
}
--- Readonly public entity snapshot for presentation. State and motion revisions are separate from entity identity.
export type BloxReplicaEntity = {
    --- Low 32 bits of the server-assigned 64-bit entity ID. Keep paired with id_hi; pass both unchanged to entity services.
    id_lo: number,
    --- High 32 bits of the same entity ID as id_lo. This is not a second ID or a revision; do not combine the pair into a Luau number.
    id_hi: number,
    --- Namespaced registered entity definition key; use it to select presentation behavior.
    key: string,
    --- Low 32 bits of the captured state revision. A version of state, not an entity identity or simulation tick.
    revision_lo: number,
    --- High 32 bits of the same captured state revision. Keep paired with revision_lo when comparing versions.
    revision_hi: number,
    --- Low 32 bits of the motion revision, separate from state revision. Anchored entities have zero motion revision.
    motion_revision_lo: number,
    --- High 32 bits of the motion revision. Keep paired with motion_revision_lo; anchored entities have both words zero.
    motion_revision_hi: number,
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
    event: "replica:entities" | "replica:anchors",
    --- Local presentation value; replica events supply an empty string.
    value: string,
    --- Local presentation state supplied to this callback; not authoritative gameplay state.
    state: string,
    --- Local presentation text map supplied to this callback.
    texts: {[string]: string},
    --- Current bounded public entity window. Do not treat this subset as every entity in the world.
    entities: {BloxReplicaEntity},
    --- Exact IDs entering this callback's tracked window. Keep each id_lo/id_hi pair together.
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
    | { op: "visual", id_lo: number, id_hi: number, yaw: number, bob: number, squash: number }
    | { op: "tint", id_lo: number, id_hi: number, r: number, g: number, b: number }
    | { op: "ember", id_lo: number, id_hi: number, x: number, y: number, z: number }
    | { op: "spark", id_lo: number, id_hi: number, x: number, y: number, z: number,
        r: number, g: number, b: number, size: number?, lifetime_ms: number? }
