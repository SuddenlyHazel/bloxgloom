# Registered actions and composed controls

`bloxgloom_host_api::actions` is the runtime-neutral declaration and discovery
surface. Extensions call `Registrar::action(Action)` at startup; the host resolves
references before installing a candidate catalog. `Registry::discover(Target)`
returns descriptors in canonical action-key order, independent of package order.
Duplicate keys and capacity overflow reject registration, not silently replace or
truncate it. The frozen action descriptors (including schemas, recipes and UI)
participate in content-map/handshake compatibility via the player contract.

## Supported targets and effects

* `Item(key)` and `Empty` offer explicit finite inventory recipes. Empty means an
  actor-local action with no world target, not a promise that a ray misses terrain.
  The selected slot must contain the declared plain input. Consumption and output
  insertion are one inventory/WAL transition. Full output, insufficient input,
  component-bearing input, or stale inventory revision reject the entire action.
* `Block(key)` offers the registered inventory screen or a fixed request to the
  anchored entity's existing registered interaction policy. Anchor cells are
  discoverable; inventory and anchored lifecycle descriptors also expose their
  declared footprint cells. `AnchoredBlockEntity::interaction` automatically
  registers its default use action (empty means no default use control).
* `Entity(key)` offers a fixed request to a registered anchored/mobile own-state policy.
  Anchored requests require the exact advertised revision and target cell. Mobile
  own-state use resolves the stable entity identity against its current position
  and state: an older movement-frame revision is allowed (zero/future revisions
  are rejected). The host validates current reach, subscription and visibility,
  and fences the newly captured authoritative revision through commit. This lets
  a click reach a continuously moving creature without accepting a replacement
  entity or bypassing the actor-inventory fence. The policy still owns its bytes.

Block-bound actions take discovery precedence over entity-type actions on an
anchor; item-bound actions similarly precede empty-space actions. Each discovery
context is independently capped at eight, rather than merging then truncating.

Operations are deliberately host capabilities, not arbitrary mutable callbacks:
`Recipe`, `EntityRequest`, and `Inventory`. Action registration cannot obtain raw
server state, write a journal, move another player's inventory, or bypass entity
slot permissions. Item harvest/loot, arbitrary world edits, commands and keybinding
registration remain separate surfaces.

## Production path and authority

Client targeting uses the connection catalog's discovery index. Existing container
opening, slot controls, and R/F keyboard shortcuts resolve registered inventory
actions; mobile controls resolve registered entity actions. There is no fixture ID
in input, layout, protocol dispatch, transactions or rendering.

`actions::Request` is carried inside the existing bounded `EntityInteract` message
and its exact durable receipt. Registered dispatch resolves the descriptor and
revalidates all inputs, then reuses the existing entity-policy transaction assembler
or stages the host inventory transition. Profile serialization, WAL admission,
receipt deduplication, replication and recovery are unchanged. Inventory and entity
state never change just because a panel opened or a worker completed.

The actor position is sampled at planning. Visibility ray traversal is reach-bounded;
missing terrain requests a load and defers. Every sight chunk is retained as a shared
transaction read fence, including chunks outside the policy footprint/read view.
Those fences survive admission until confirmed apply and prevent an intervening
terrain commit from invalidating the visibility decision. Rejected stale requests
need a newly discovered/current request and a new action sequence; resending the
same sequence only replays its receipt. Loading/admission conflicts remain eligible
for the existing durable queue's retry behavior. No new scheduling queues exist.

Legacy identity-fenced inventory/mobile/anchored requests are resolved through the same
registry. Unfenced inventory-v1 requests are rejected (including old R/F bytes).
The network frame format and inventory encoding do not change; the action envelope
uses payload tag 5 (tag 4 is reserved for anchored lifecycle requests). Catalog fingerprint changes require the parent's prerelease
world-version bump rather than a format converter.

## Bounded composition

A generic `Actions` host screen renders registered `Panel` declarations. Supported
widgets are labels and action buttons with tooltips; panels can be attached to
item, empty-space, block and mobile actions. Buttons emit the registered versioned
request, not client-authored effects. A button can reference another registered
action in the same target context; missing/incompatible references reject the
catalog before startup. Standard inventory grids/status remain in
`InventoryScreen`; arbitrary canvas/GPU or scripting widgets are not promised.

Authored gameplay block requests use tag 6, which wraps the versioned action
request with the observed streamed chunk version. The server requires a match
against its resident chunk at planning and retains its read fence through
admission; remove-and-restore of the same block type is still stale. A client
version cannot authorize an edit. Other targets retain tag 5 or their existing
identity-fenced envelopes.

Bounds: 256 action definitions, 8 actions per target, 8 widgets per panel, 128-byte
action keys, 239-byte fixed policy requests, 4 bytes of inventory-control arguments.
The canonical registered request codec allows up to 130 argument bytes for
gameplay actions (enough for a two-byte count plus a 128-byte content key),
but the entire request—including a nine-byte chunk-version wrapper when used—
must fit the existing 256-byte interaction limit. Longer arguments do not
authorize a command: argument schemas and permission descriptors are still
pending. Decoding rejects truncation/trailing bytes before retaining arguments,
and discovery scans at most the fixed registry capacity. Inventory work touches
the 36 host slots and preserves the 128-per-stack cap. Text is printable ASCII
with per-field lengths.

The independent fixture's `fixture:knap` action consumes two plain gravel and
produces three sticks. The loopback test uses its actual composed client control,
the production nonblocking listener, duplicate/stale requests, confirmed inventory
replication and restart. It is a proof of explicit item consumption/production,
not a new builtin recipe or a complete harvest/loot API.
