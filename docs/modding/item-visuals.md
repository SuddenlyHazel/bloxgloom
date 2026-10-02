# Luau item icons and stack presentation

Server startup can register native bitmap art for an owned item, requiring
`bloxgloom:content/v1`. Register the item before installation completes:

```lua
host.register_item_icon('example:battery', {
    rows = {'..xx..', '.xxxx.', '.x..x.', '.xxxx.'},
    palette = {x = {0.8, 0.9, 0.2, 1}},
})
```

Icons have 1–32 ASCII rows, 1–32 pixels per row and at most 32 palette symbols.
A dot is transparent. RGBA channels must be finite and between zero and one.
A package can declare 32 icons. Invalid or caught registration failures reject
startup. Frozen art contributes to catalog identity and is delivered as validated
V47 client metadata; server sources are not delivered.

A format-2 client/shared `client_startup` module can bind an owned callback module:

```lua
return function(host)
    host.set_item_visual_handler('example:battery', 'example:battery_visual')
end
```

The callback receives one read-only stack snapshot and returns data:

```lua
return function(stack)
    local charged = string.byte(stack.components, 1) == 255
    return {
        icon = {
            rows = {'xx', 'xx'},
            palette = {x = charged and {0.2, 0.9, 0.3, 1} or {0.9, 0.4, 0.2, 1}},
        },
        drop_scale = 0.5 + stack.count / 128,
    }
end
```

Inputs are `item` (namespaced key), `count` (1–128), `component_version`
(zero when absent), and `components` (exact opaque bytes as a Luau string).
Inventory slots, the hotbar and machine/container slots use the returned bitmap.
The optional `drop_scale` must be 0.5–1.5 and multiplies ordinary world-drop art;
it does not alter collisions, pickup, ownership or inventory. World-drop snapshots
currently carry counts but no component bytes, so dropped-item callbacks receive
an empty component string. UI stacks carry their exact negotiated components.

Callbacks run on a connection-owned worker, with isolated attempts, declared
client imports, no world mutation or GPU access, 8 MiB memory, 2,000 interrupt
checks and 20 ms per attempt. Each package can bind 32 owned item/module pairs.
Render calls never wait for script execution: a bounded cache keeps 512 stack
variants with at most 64 queued requests, deduplicates pending requests, and evicts
completed entries by last use. Until a reply arrives, and after errors, ordinary
registered icons and drop scale remain available. A failed variant is cached to
avoid retrying it each frame. Callbacks should depend only on their stack input.

`fixtures/item-visuals/packages` demonstrates orange low-count, green full-count,
and blue component-dependent cell art, plus count-dependent world-drop scale.
The focused egui paint test can produce a headless image:

```sh
BLOXGLOOM_ITEM_ICON_PREVIEW=/tmp/item-icon.png cargo test item_visuals_egui_slots
```
