-- Editor-only callback types for Bloxgloom package modules. Annotate callback
-- arguments explicitly; these are not globals or a replacement for host validation.

export type BloxStartupHost = {
    register_texture: (key: string, declared_asset: string) -> (),
    register_block: (key: string, name: string, package_texture: string) -> (),
    register_item: (key: string, name: string, texture: string) -> (),
    register_generator: (key: string, revision: number, module: string) -> (),
    register_action: (key: string, revision: number, label: string, target_kind: "empty" | "item" | "block", target_key: string?, module: string) -> (),
    register_handler: (key: string, revision: number, event: "BlockRemoved" | "BlockPlaced" | "NeighborChanged" | "EntityTick" | "PickupRequested", target: string, module: string) -> (),
    register_entity: (key: string, schema_version: number, state_bytes: number, public_bytes: number, initial_delay_ticks: number?) -> (),
    register_system: (declaration: BloxSystemDeclaration) -> (),
}

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
    seeds: { BloxSystemSeed },
}

export type BloxGenerationContext = {
    seed_lo: number,
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
export type BloxTriple = { number }
export type BloxActionEvent = {
    kind: "ActionRequested",
    action: string,
    position: BloxTriple,
    cell: BloxTriple?,
    slot: number,
    arguments: string,
    entity_lo: number?,
    entity_hi: number?,
}
export type BloxGameplayContext = {
    tick_lo: number,
    tick_hi: number,
    block: (x: number, y: number, z: number) -> BloxBlock,
    set_block: (x: number, y: number, z: number, state_key: string) -> (),
    transfer: (from_slot: number, to_slot: number, count: number) -> boolean,
    spawn_drop: (x: number, y: number, z: number, item: string, count: number, delay_ms: number) -> (),
    spawn_entity: (key: string, x: number, y: number, z: number, state: string) -> (),
    entity_state: (id_lo: number, id_hi: number) -> string?,
    update_entity: (id_lo: number, id_hi: number, state: string) -> boolean,
    remove_entity: (id_lo: number, id_hi: number) -> boolean,
    schedule_entity: (id_lo: number, id_hi: number, delay_ticks: number?) -> boolean,
    inventory: (owner: BloxInventoryOwner) -> { BloxSlot },
    give: (owner: BloxInventoryOwner, stack: BloxStack) -> boolean,
    take: (owner: BloxInventoryOwner, slot: number, count: number) -> BloxStack?,
    transfer_inventory: (from: BloxInventoryOwner, source: number, to: BloxInventoryOwner, destination: number, count: number) -> boolean,
}
export type BloxInventoryOwner = "player" | { entity_lo: number, entity_hi: number }
export type BloxStack = { item: string, count: number, components: { version: number, bytes: string }? }
export type BloxSlot = { stack: BloxStack?, insert: boolean, extract: boolean }

export type BloxOwnerContext = {
    owner: BloxTriple,
    data: string,
    tick_lo: number,
    tick_hi: number,
    revision_lo: number,
    revision_hi: number,
    inbox: { BloxOwnerIntentDelivery },
    block: (x: number, y: number, z: number) -> string,
    edit: (x: number, y: number, z: number, before: string, after: string) -> (),
    wake: (system: string, x: number, y: number, z: number) -> (),
    send: (x: number, y: number, z: number, binary_payload: string) -> (),
}
export type BloxOwnerIntentId = {
    source: BloxTriple,
    revision_lo: number,
    revision_hi: number,
    ordinal: number,
}
export type BloxOwnerIntentDelivery = {
    id: BloxOwnerIntentId,
    produced_tick_lo: number,
    produced_tick_hi: number,
    payload: string,
}

export type BloxClientStartupHost = {
    set_text: (owned_document_node: string, ascii_text: string) -> (),
    set_state: (owned_document: string, ascii_state: string) -> (),
}

export type BloxUiInput = {
    sequence: number,
    event: string,
    value: string,
    state: string,
    texts: { [string]: string },
}
export type BloxUiCommand =
    { op: "text", node: string, value: string }
    | { op: "visible", node: string, value: boolean }
    | { op: "state", value: string }
    | { op: "action", key: string }
