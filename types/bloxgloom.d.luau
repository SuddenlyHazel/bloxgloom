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
    model: string?,
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

--- Frozen tag expansions. Unknown tags fail the callback, even if caught with pcall.
export type BloxTagQueries = {
    tag_contains: (kind: "block" | "item", tag: string, member: string) -> boolean,
    --- Zero-based offset, 1..128 page size; returns sorted keys and total membership.
    tag_members: (kind: "block" | "item", tag: string, offset: number?, limit: number?) -> ({string}, number),
}
--- Historical environment captured before scheduled work. This is not a direct-sunlight query.
export type BloxEnvironmentQueries = {
    world_time: () -> {elapsed_ms: number, cycle_ms: number},
    weather: () -> BloxWeather,
}
export type BloxItemIcon = {rows: {string}, palette: {[string]: {number}}}
export type BloxItemVisualInput = {item: string, count: number, component_version: number, components: string}
export type BloxItemVisual = {icon: BloxItemIcon?, drop_scale: number?}
export type BloxItemVisualCallback = (BloxItemVisualInput) -> BloxItemVisual

--- Startup registration services. Definitions are frozen before gameplay; methods use dot calls.
--- Registration methods expire when startup returns; never retain this host.
export type BloxStartupHost = {
    --- Eight/package; self-contained GLB and optional JSON controls declared in package.txt.
    register_model: (declaration: BloxModelDeclaration) -> (),
    register_player_model: (declaration: BloxPlayerModelDeclaration) -> (),
    register_region: (key: string, minimum: BloxTriple, maximum: BloxTriple, own_service: string) -> (),
    register_chat_hook: (key: string, revision: number, module: string) -> (),
    register_damage_policy: (key: string, revision: number, own_module: string) -> (),
    register_health_hook: (key: string, revision: number, own_module: string) -> (),
    --- At most 256/package; encoded and decoded asset limits remain independent.
    register_texture: (key: string, declared_asset: string, options: BloxTextureOptions?) -> (),
    --- At most 256/package; each block also consumes one of 512 total item slots.
    register_block: (key: string, name: string, package_texture: string, options: BloxBlockOptions?) -> (),
    --- At most 512 total/package, including all block-created items.
    register_item: (key: string, name: string, texture: string, options: BloxItemOptions?) -> (),
    register_item_icon: (own_item: string, icon: BloxItemIcon) -> (),
    -- A member is a namespaced definition key or "#namespace:tag".
    -- At most 32 tags per package and 32 members per tag; cycles fail startup.
    register_tag: (key: string, kind: "block" | "item", members: {string}) -> (),
    --- Eight distinct owned keys/package; lexical key order determines overlap precedence.
    register_generator: (key: string, revision: number, module: string) -> (),
    -- Optional command facet: empty-target only; action key is its name.
    register_action: (key: string, revision: number, label: string, target_kind: "empty" | "item" | "block" | "entity", target_key: string?, module: string, command: BloxCommand?) -> (),
    -- Readonly advisory post-WAL callback; actions/v1, at most eight/package.
    register_weather_observer: (key: string, revision: number, module: string) -> (),
    register_committed_observer: (key: string, revision: number, module: string) -> (),
    register_handler: (key: string, revision: number, event: "BlockRemoved" | "BlockPlaced" | "NeighborChanged" | "EntityTick" | "PickupRequested", target: string, module: string) -> (),
    register_entity: (key: string, schema_version: number, state_bytes: number, public_bytes: number, initial_delay_ticks: number?) -> (),
    --- Eight distinct owned keys/package; state and deadlines remain independent by system.
    register_system: (declaration: BloxSystemDeclaration) -> (),
    -- Requires players/v1; own-package key/module, revision 1..65535, private state cap 1..4096.
    register_player_lifecycle: (key: string, revision: number, max_state_bytes: number, initial_state: string, module: string) -> (),
    register_creature: (declaration: BloxCreatureDeclaration) -> (),
    --- Requires content/v1 + actions/v1 + moving_entities/v1; at most eight/package.
    register_moving_entity: (declaration: BloxMovingDeclaration) -> (),
    register_machine: (declaration: BloxMachineDeclaration) -> (),
    --- Requires content + anchored_entities/v1. Pure own-state server behavior; at most eight/package.
    register_anchored: (declaration: BloxAnchoredDeclaration) -> (),
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
--- General anchored geometry uses the full native 64-cell/±16-coordinate bounds.
export type BloxAnchoredFootprintCell = {offset: BloxTriple, state: string?}
export type BloxAnchoredDeclaration = {
    entity: string, block: string, module: string,
    schema_version: number, max_state_bytes: number, max_public_bytes: number,
    interval: number,
    placement_item: string?, anchor_state: string?,
    placement_cost: number?, removal_refund: number?,
    footprint: {BloxAnchoredFootprintCell}?, observe: {BloxTriple}?, interaction: string?,
}
export type BloxAnchoredCell = {offset: BloxTriple, state: string, solid: boolean}
--- All event tables/sequences are readonly; no live host APIs are given.
export type BloxAnchoredEvent =
    {kind: "Initialize", anchor: BloxTriple}
    | {kind: "Validate", state: string}
    | {kind: "Public", state: string}
    | {kind: "Interact", state: string, request: string}
    | ({kind: "React", state: string, anchor: BloxTriple, tick: BloxTick, cells: {BloxAnchoredCell}} & BloxTagQueries & BloxEnvironmentQueries)
    | {kind: "Refund", state: string, cause: "Broken" | "Reaction" | "WorldEdit", maximum: number}
--- Initialize/Interact return private bytes; Public returns public bytes.
--- Validate returns true/nil or false; React returns nil, {state=bytes} or {remove=true}.
--- Refund returns integer 0..maximum or nil for the configured maximum.
export type BloxAnchoredReply = string | boolean | number | {state: string} | {remove: boolean} | nil

export type BloxStorageOptions = {
    footprint: {BloxTriple}?,
    hint: string?,
    --- Ordered groups must cover every slot exactly once.
    groups: {{label: string, count: number}}?,
}

export type BloxModelDeclaration = {
    key: string, asset: string, controls: string?, scale: number?,
}
export type BloxPlayerModelDeclaration = BloxModelDeclaration & {
    clips: {idle:string?,walk:string?,run:string?,crouch:string?,tool_left:string?,tool_right:string?}?,
    crossfade_s: number?, first_person_hide: {string}?, first_person_offset: BloxTriple?,
}
export type BloxPlayerHealth = {current:number,max:number,alive:boolean,revision:BloxRevision,life:BloxTick}
export type BloxPlayerDamage = {player:BloxPlayerView,health:BloxPlayerHealth,amount:number,cause:string}
--- Pure immutable decision. Return integer 0..1,000,000; zero cancels.
export type BloxPlayerDamageCallback = (event:BloxPlayerDamage) -> number
export type BloxPlayerHealthEvent = {kind:"died"|"respawned",player:BloxPlayerView,before:BloxPlayerHealth,health:BloxPlayerHealth,cause:string?}
--- Transaction-bound hook; health mutations are forbidden recursively.
export type BloxPlayerHealthCallback = (context:BloxGameplayContext,event:BloxPlayerHealthEvent) -> ()
export type BloxPlayerModifierOptions = {
    speed: number?, sprint: number?, jump: number?, gravity: number?, duration_ticks: number?,
}
export type BloxPlayerModifier = {
    key: string, speed: number, sprint: number, jump: number, gravity: number, expires_at: BloxTick?,
}
export type BloxChatRequest = {
    sender: BloxPlayerView, text: string, players: {BloxPlayerView}, identity_trust: "claimed_profile",
}
export type BloxChatDecision = {allow:true,text:string,recipients:{BloxPlayerView}?} | {allow:false,reason:string}
export type BloxChatModerator = (BloxChatRequest) -> BloxChatDecision
export type BloxAuthoredCreatureModel = {
    key: string, scale: number?, idle: string?, walk: string?, run: string?,
}
--- Ground creature declaration. Model registration precedes the creature; collision uses body.
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
    model: string | BloxAuthoredCreatureModel | {{min: BloxTriple, max: BloxTriple, color: BloxTriple,
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

export type BloxImpactProfile = {
    gain: number, -- 0..2
    click: number, -- 0..2
    frequency_hz: {number}, -- exactly two, 50..16000
    damping_per_s: {number}, -- exactly two, 10..2000
    resonance: number, -- 0..2
    lowpass_hz: number, -- 100..18000
}
export type BloxAcoustics = {
    surface: "water" | "dirt" | "leaf" | "stone" | "concrete" | "glass" | "metal" | "plastic" | "asphalt" | "roof" | "wood",
    habitat: ("none" | "ground" | "canopy")?,
    impact: BloxImpactProfile?,
}
export type BloxWeather = {
    kind: "clear" | "rain" | "storm" | "storm_mild" | "storm_severe",
    revision_lo: number, revision_hi: number,
    elapsed_ms_lo: number, elapsed_ms_hi: number,
    rain_mm_h: number, wind_m_s: number, cloud: number, transition: number,
}
export type BloxWeatherChanged = {kind: "WeatherChanged", previous: BloxWeather, current: BloxWeather}
export type BloxBlockOptions = {
    acoustics: BloxAcoustics?,
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
-- Text: nonempty UTF-8 without control characters, max_bytes=1..128.
-- Integer: inclusive bounds within +/-9007199254740991 (exact Luau numbers).
-- Number: inclusive finite bounds; NaN/infinity are rejected, -0 normalized.
-- Counts: one byte, 1..128. Defaults make trailing count arguments optional;
-- wire/event.arguments always includes every field with explicit default values.
export type BloxCommandArgument = { kind: "item_key" | "entity_key", max_bytes: number }
    | { kind: "text", max_bytes: number }
    | { kind: "integer" | "number", min: number, max: number }
    | { kind: "count", default: number? }
    | { kind: "player" }
-- At most four globally unique aliases, each 1..32 lowercase ASCII letters,
-- digits or underscores. Builtin command names are reserved. Aliases resolve
-- on the client to the canonical namespaced action; they are not wire identities.
-- Example descriptor: {permission="Admin", aliases={"tune"}, arguments={
--     {kind="text", max_bytes=48}, {kind="integer", min=-10, max=10},
--     {kind="number", min=0, max=1}}}
-- Command entry: tune "rain intensity" -2 0.5
export type BloxCommand = {
    permission: "Player" | "Admin",
    arguments: { BloxCommandArgument }?,
    aliases: { string }?,
}

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
    creates_moving_entities: boolean?, -- requires read_world and moving_entities/v1
    mutates_motion: boolean?, -- requires reads_entities and moving_entities/v1
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

export type BloxQueriedBlock = BloxBlock & {cell: BloxTriple}
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
    cause: "Break" | "Replacement" | "SupportLoss" | "WorldEdit" | "Burn" | "AnchoredBreak" | "Transformation",
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
export type BloxQuaternion = {number} -- dense [x,y,z,w], unit length
export type BloxMotion = {
    position: BloxTriple, velocity: BloxTriple, acceleration: BloxTriple,
    orientation: BloxQuaternion, angular_velocity: BloxTriple, revision: BloxRevision, grounded: boolean,
}
export type BloxMovingTarget = {kind: "Terrain", cell: BloxTriple, state: string}
    | {kind: "Entity", entity: BloxEntityId, revision: BloxRevision}
export type BloxMotionContact = {
    motion_revision: BloxRevision, tick: BloxTick,
    target: BloxMovingTarget, normal: BloxTriple,
}
export type BloxMovingTickEvent = {
    kind: "MovingTick", entity: BloxEntityId, tick: BloxTick, motion: BloxMotion,
}
export type BloxMovingImpactEvent = {
    kind: "MovingImpact", entity: BloxEntityId, motion_revision: BloxRevision, tick: BloxTick,
    position: BloxTriple, normal: BloxTriple, incoming_velocity: BloxTriple,
    target: BloxMovingTarget, blocked: boolean,
}
export type BloxMovingExpiryEvent = {
    kind: "MovingExpiry", entity: BloxEntityId, motion_revision: BloxRevision, tick: BloxTick,
    reason: "Lifetime" | "WorldBoundary",
}
export type BloxSpawnReference = {__brand: "BloxSpawnReference", ordinal: number}
export type BloxMovingSpawn = {
    position: BloxTriple, velocity: BloxTriple?, orientation: BloxQuaternion?, state: string,
    source: BloxEntityId?, angular_velocity: BloxTriple?,
}
export type BloxMotionChange = {
    velocity: BloxTriple?, acceleration: BloxTriple?, orientation: BloxQuaternion?, angular_velocity: BloxTriple?,
}
export type BloxMovingDeclaration = {
    key: string, module: string, schema: number, revision: number,
    max_state_bytes: number, max_public_bytes: number, interval: number, lifetime_ticks: number,
    source_exclusion_ticks: number?, handles_impact: boolean?, handles_expiry: boolean?,
    body: {
        origin: "center"?, half_extents: BloxTriple,
        collisions: {terrain: boolean?, players: boolean?, creatures: boolean?}?,
        response: ("stop" | "bounce" | "slide")?, restitution: number?, gravity_scale: number?,
        max_speed: number, max_acceleration: number,
    },
    model: {{min: BloxTriple, max: BloxTriple, color: BloxTriple, motion: "body"?}}?,
    physics: {linear_damping: number?, angular_damping: number?, friction: number?, max_angular_speed: number?}?,
}
--- Match kind before accessing event-specific fields.
export type BloxGameplayEvent = BloxActionEvent | BloxRemovedEvent | BloxPlacedEvent
    | BloxNeighbourEvent | BloxEntityTickEvent | BloxPickupEvent
    | BloxMovingTickEvent | BloxMovingImpactEvent | BloxMovingExpiryEvent

--- Server transaction services. Proposed mutations commit together; an error rejects the transaction.
export type BloxProfileCell = {
    revision: BloxRevision, initialized: boolean, state: string, public_state: string, next_tick: BloxTick?,
}
--- Invocation-bound authority. Saved methods expire after this attempt.
--- Authoritative module state is fresh on retries; persist decisions through host state.
export type BloxGameplayContext = BloxTagQueries & {
    --- Explicit canonical-token decoding; a profile handle grants no authority.
    profile_id: (canonical_token: string) -> BloxProfileId,
    --- Own registered service, players/v1. Captures live or offline profile revision; readonly binary values.
    profile_state: (own_service: string, profile: BloxProfileId) -> BloxProfileCell,
    --- Ordinary actions/admitted lifecycle callbacks. Atomic with rewards; retains the profile timer.
    set_profile_state: (own_service: string, profile: BloxProfileId, state: string, public_state: string) -> (),
    player_profile: BloxProfileId?,
    players: () -> {BloxPlayerView},
    player_by_profile: (profile: BloxProfileId) -> BloxPlayerView?,
    player_by_session: (session: BloxSessionId) -> BloxPlayerView?,
    --- Requires the executing package's players/v1 capability. Exact online session; after receipt, no replay.
    message_player: (session: BloxSessionId, text: string) -> (),
    kick_player: (session: BloxSessionId, reason: string) -> (),
    --- Profile effects persist; exact-session effects vanish on leave. Duration uses logical ticks.
    set_player_modifier: (target: BloxProfileId | BloxSessionId, own_key: string, options: BloxPlayerModifierOptions) -> (),
    remove_player_modifier: (target: BloxProfileId | BloxSessionId, own_key: string) -> (),
    player_modifiers: (target: BloxProfileId | BloxSessionId) -> {BloxPlayerModifier},
    player_health: (target:BloxSessionId) -> BloxPlayerHealth,
    --- players/v1, exact online session/current revision. Amounts are integers 1..1,000,000.
    damage_player: (target:BloxSessionId,revision:BloxRevision,amount:number,own_cause:string) -> (),
    heal_player: (target:BloxSessionId,revision:BloxRevision,amount:number) -> (),
    set_player_max_health: (target:BloxSessionId,revision:BloxRevision,max:number) -> (),
    --- Dead target only; final authoritative terrain must be loaded, supported and unobstructed.
    respawn_player: (target:BloxSessionId,revision:BloxRevision,x:number,y:number,z:number) -> (),
    set_player_model: (session: BloxSessionId, key: string?) -> (),
    set_player_model_variant: (session: BloxSessionId, group: string, option: string?) -> (),
    set_player_model_layer: (session: BloxSessionId, layer: string, visible: boolean?) -> (),
    set_player_model_tint: (session: BloxSessionId, tint: string, value: {rgb:BloxTriple,mode:"multiply"|"replace"}?) -> (),
    play_player_animation: (session: BloxSessionId, value: {clip:string,speed:number?,looping:boolean?,crossfade_ms:number?}) -> (),
    stop_player_animation: (session: BloxSessionId, crossfade_ms: number?) -> (),
    --- Registered integer palette indices, players/v1 authority, exact session. Native cosmetic save precedes replication.
    set_player_appearance: (session: BloxSessionId, skin: number, shirt: number, pants: number) -> (),
    --- Absolute feet position, players/v1 authority. Host fences final terrain and resets old movement input/prediction.
    teleport_player: (session: BloxSessionId, x: number, y: number, z: number) -> (),
    --- Available in gameplay action callbacks; captured phase, with read-your-writes.
    --- One captured historical weather view. Staged overrides appear after commit.
    weather: () -> BloxWeather,
    --- Authenticated operator only; duration integer 0..60000 ms, same durable transaction.
    admin_set_weather: (kind: "clear" | "rain" | "storm" | "storm_mild" | "storm_severe", transition_ms: number) -> (),
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
    --- Integer dimensions, at most 64 cells; unknown terrain rejects the callback.
    blocks: (x: number, y: number, z: number, width: number, height: number, depth: number) -> {BloxQueriedBlock},
    set_block: (x: number, y: number, z: number, state_key: string) -> (),
    --- Stage an exact-count move between 0-indexed player inventory slots. Returns false if the move cannot be made.
    transfer: (from_slot: number, to_slot: number, count: number) -> boolean,
    --- Presentation publishes only after this gameplay transaction commits. Invalid calls latch rollback.
    sound: (event: BloxSound) -> (),
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    --- Opaque attempt-local allocation reference; this is not an entity ID.
    spawn_moving_entity: (key: string, options: BloxMovingSpawn) -> BloxSpawnReference,
    --- Replace a staged launch in its originating transaction; key must be unchanged.
    cancel_moving_entity: (id: BloxEntityId) -> boolean,
    configure_spawn: (reference: BloxSpawnReference, key: string, options: BloxMovingSpawn) -> (),
    --- Captured owned motion; nil when unavailable. Nested values are readonly.
    motion: (id: BloxEntityId) -> BloxMotion?,
    motion_contact: (id: BloxEntityId) -> BloxMotionContact?, -- readonly captured contact, nil when free
    --- Revision-fenced steering, without a position/teleport operation.
    set_motion: (id: BloxEntityId, expected_revision: BloxRevision, options: BloxMotionChange) -> boolean,
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
--- Other profiles require package players/v1 in actions/post-admission lifecycle.
--- An invocation captures its actor and at most seven other profile inventories.
export type BloxInventoryOwner = "player" | BloxEntityId | BloxProfileId
--- An item stack. Count is 1..128; component bytes follow the item's registered schema.
export type BloxStack = { item: string, count: number, components: { version: number, bytes: string }? }
--- An inventory slot. A missing stack means empty; insert/extract describe permitted transfers.
export type BloxSlot = { stack: BloxStack?, insert: boolean, extract: boolean }

--- Durable owner planner input. Owner identity is selected by owner_kind; mutations use captured revisions.
export type BloxOwnerContext = BloxTagQueries & BloxEnvironmentQueries & {

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
    blocks: (x: number, y: number, z: number, width: number, height: number, depth: number) -> {BloxQueriedBlock},
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    spawn_moving_entity: (key: string, options: BloxMovingSpawn) -> BloxSpawnReference,
    configure_spawn: (reference: BloxSpawnReference, key: string, options: BloxMovingSpawn) -> (),
    motion: (id: BloxEntityId) -> BloxMotion?,
    motion_contact: (id: BloxEntityId) -> BloxMotionContact?, -- readonly captured contact, nil when free
    set_motion: (id: BloxEntityId, expected_revision: BloxRevision, options: BloxMotionChange) -> boolean,
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
    --- Read-only stack presentation runs off the window thread; count-only for world drops.
    set_item_visual_handler: (own_item: string, own_client_module: string) -> (),
    --- Set text on an owned document/node identity, such as "demo:controls/title".
    set_text: (owned_document_node: string, ascii_text: string) -> (),
    --- Set explicit local state for an owned document. This is presentation state, not saved gameplay state.
    set_state: (owned_document: string, ascii_state: string) -> (),
    --- Set a declared parameter on an owned material/effect. Its declared type and bounds are checked by the host.
    set_parameter: (owned_resource: string, parameter_name: string, value: BloxVisualParameterValue) -> (),
    --- Select an owned client module for bounded public replica presentation callbacks in this session.
    --- observations=true adds inventory/block/world/action notifications.
    --- Default false preserves existing entity/anchor callback behavior.
    set_replica_handler: (own_client_module: string, observations: boolean?) -> (),
    --- Requires players/v1. One own-package module; multiple packages compose.
    set_player_handler: (own_client_module: string) -> (),
}

--- Callback host: presentation only. Registrations are startup-only.
--- Methods expire when this callback returns. Retained controllers must receive a fresh host.
--- Module locals/coroutines survive in this handler's connection realm, separate from UI.
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
--- Readonly authoritative replica data already installed on this client.
--- Unknown inventory/world values are nil; a zero inventory revision is valid.
export type BloxReplicaSnapshot = {
    inventory: BloxReplicaInventory?,
    world: BloxReplicaWorld?,
    weather: BloxWeather?,
    --- Up to 64 recent installed block observations. No procedural fallback.
    blocks: {BloxReplicaBlock},
    blocks_truncated: boolean,
    --- Up to 16 own-session receipts scoped to this callback's package.
    actions: {BloxReplicaAction},
}
--- Own player's installed finite inventory; no other profiles or containers.
export type BloxReplicaInventory = {
    revision: BloxRevision,
    revision_lo: number,
    revision_hi: number,
    --- Dense 36-entry sequence. slot is zero-based; stack is nil for empty slots.
    slots: {BloxReplicaInventorySlot},
}
export type BloxReplicaInventorySlot = {slot: number, stack: BloxStack?}
--- Latest received server clock sample; presentation interpolation is excluded.
export type BloxReplicaWorld = {elapsed_ms: number, cycle_ms: number}
--- Bounded recent installed block observations, rather than arbitrary terrain access.
export type BloxReplicaBlock = {
    position: {x: number, y: number, z: number},
    state: string,
    revision: BloxRevision,
    revision_lo: number,
    revision_hi: number,
}
--- Recent own-session receipts. id is the exact canonical 32-hex-digit action ID.
export type BloxReplicaAction = {id: string, key: string?, accepted: boolean, reason: string}

--- Captured readonly data for this callback, not a live world view.
--- Module locals may survive events in this entry's connection presentation realm.
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
    --- Stable widget identity for widget events; nil for declared input actions.
    node: string?,
    --- Typed current value for an editable widget event.
    value_typed: BloxUiValue?,
    --- Readonly map of editable widget values, keyed by stable widget identity.
    values: {[string]: BloxUiValue},
    replica: BloxReplicaSnapshot,
    spawned: {{ordinal: number, entity: BloxEntityId}}, -- zero-based launch ordinal after receipt
}
--- Presentation updates or one gameplay action request. Action targets come from the current selection/aim and are checked by the server.
export type BloxUiValue = boolean | number | string
--- Dynamic forest roots attach to the target container; parent indices are zero-based within this array.
export type BloxUiNode = {
    id: string, parent: number?, kind: "panel" | "scroll_panel" | "table" | "label" | "image" | "button" | "input" | "multiline_input" | "checkbox" | "slider" | "select",
    style: string, text: string?, image: string?, event: string?,
    checked: boolean?, value: number?, min: number?, max: number?, step: number?,
    options: {{key: string, label: string}}?, selected: string?,
}
export type BloxUiCommand =
    BloxSoundCommand
    | { op: "text", node: string, value: string }
    | { op: "value", node: string, value: BloxUiValue }
    | { op: "children", node: string, nodes: {BloxUiNode} }
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
    --- Typed control values for the active document owned by this package; otherwise empty.
    values: {[string]: BloxUiValue},
    replica: BloxReplicaSnapshot,
    --- Current bounded public entity window. Do not treat this subset as every entity in the world.
    entities: {BloxReplicaEntity},
    --- Exact IDs entering this callback's tracked window. Use each record's id handle.
    entered: {BloxReplicaId},
    --- Exact IDs leaving this callback's tracked window. Leaving the window does not necessarily mean despawn.
    left: {BloxReplicaId},
}
--- Local presentation updates. Entity commands must reference an entity in the current replica window; they do not change gameplay.
--- Named voices are scoped to the entry package and server/client side. Loops require an entity.
export type BloxSound =
    { kind: "play", voice: string, clip: string, position: BloxTriple?, entity: BloxEntityId?,
      gain: number?, pitch: number?, looping: boolean? }
    | { kind: "update", voice: string, position: BloxTriple?, gain: number?, pitch: number? }
    | { kind: "stop", voice: string }
export type BloxSoundCommand =
    { op: "sound", kind: "play", voice: string, clip: string, position: BloxTriple?, entity: BloxEntityId?,
      gain: number?, pitch: number?, looping: boolean? }
    | { op: "sound", kind: "update", voice: string, position: BloxTriple?, gain: number?, pitch: number? }
    | { op: "sound", kind: "stop", voice: string }

export type BloxReplicaCommand =
    BloxSoundCommand
    | { op: "text", node: string, value: string }
    | { op: "value", node: string, value: BloxUiValue }
    | { op: "children", node: string, nodes: {BloxUiNode} }
    | { op: "visible", node: string, value: boolean }
    | { op: "state", value: string }
    | { op: "parameter", resource: string, name: string, value: BloxVisualParameterValue }
    | { op: "visual", entity: BloxEntityId, yaw: number, bob: number, squash: number }
    | { op: "tint", entity: BloxEntityId, r: number, g: number, b: number }
    | { op: "ember", entity: BloxEntityId, x: number, y: number, z: number }
    | { op: "spark", entity: BloxEntityId, x: number, y: number, z: number,
        r: number, g: number, b: number, size: number?, lifetime_ms: number? }

--- Creature callback input. Tick services read captured terrain; interaction requests carry binary bytes.
export type BloxCreatureVisualControls = {
    --- Present only for a registered GLB creature. Invalid calls abort the whole callback.
    play_animation: (options: {clip: string, speed: number?, looping: boolean?, crossfade_ms: number?, restart: boolean?}) -> (),
    stop_animation: (crossfade_ms: number?) -> (),
    set_variant: (group: string, option: string) -> (),
    set_layer: (name: string, visible: boolean) -> (),
    --- RGB bytes are 0..255. Multiply preserves painted shading; replace substitutes texture RGB.
    set_tint: (name: string, color: {rgb: {number}, mode: ("multiply" | "replace")?}) -> (),
    visual: {clip: string?, speed: number?, looping: boolean?},
}
export type BloxCreatureContext = BloxCreatureVisualControls & ((BloxTagQueries & BloxEnvironmentQueries & {
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
}) | {
    event: "interact",
    data: string,
    request: string,
})
--- Public neighbour snapshot. No private state or authority to mutate the neighbour.
export type BloxCreatureNeighbour = {
    id: BloxEntityId,
    key: string,
    position: BloxTriple,
    public: string,
}
--- Machine planner input. tick and due are exact logical simulation times.
export type BloxMachineContext = BloxTagQueries & BloxEnvironmentQueries & {
    data: string,
    tick: BloxTick,
    due: BloxTick,
    fuel: number,
    progress: number,
    --- Readonly 1-indexed authoritative owned slots; nil means empty.
    slots: {BloxMachineSlot?},
}
export type BloxMachineSlot = {
    --- Namespaced registered item key.
    item: string,
    count: number,
    has_components: boolean,
    --- Exact read-only binary payload owned by this machine, never peer inventory data.
    components: BloxComponents?,
}

--- One-based own slot; count is 1..128 and cannot exceed its captured stack.
--- The host captures the full exact preimage; scripts cannot supply a trusted expectation.
export type BloxMachineTransformInput = {slot: number, count: number}
--- One-based own destination; registered item/schema and slot filters still apply.
--- Only exact component matches merge; the resulting stack cannot exceed 128.
export type BloxMachineTransformOutput = {
    slot: number, item: string, count: number, components: BloxComponents?,
}
--- Consume at least one item from 1..8 distinct input slots; 0..8 distinct outputs.
--- Dense lists. All inventory changes commit atomically, including retries/recovery.
export type BloxMachineTransformation = {
    kind: "transform",
    inputs: {BloxMachineTransformInput},
    outputs: {BloxMachineTransformOutput},
}

--- Ordered work proposals; the host resolves exact finite stacks and permitted peers.
export type BloxMachineWork = {kind: "process"} | BloxMachineTransformation | {
    kind: "transfer", offset: BloxTriple, port: string, peer_port: string?,
    push: boolean, count: number?, source_slot: number?, destination_slot: number?,
    item: string?, same_as_slot: number?,
}
export type BloxMachinePlanner = (BloxMachineContext) -> (string, number, boolean | {BloxMachineWork})

--- Captured inputs. Profile bytes persist; session bytes end with this connection.
export type BloxPlayerLifecycleEvent = {
    kind: "PlayerJoining" | "PlayerJoined" | "PlayerLeaving" | "PlayerLeft" | "PlayerSpawned" | "ProfileTick" | "SessionTick" | "RegionEntered" | "RegionLeft",
    profile: BloxProfileId, player: BloxPlayerView?, transition: BloxTick, region: string?,
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
--- Advisory historical data. Observer locals are ephemeral, not durable commit receipts.
export type BloxCommittedEvent = {
    kind: "Committed",
    blocks: {{cell: {number}, state: string}},
    entities: {BloxCommittedEntity},
    inventory: {profile: BloxProfileId, revision: BloxRevision}?,
}
