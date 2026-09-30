# Luau authoring in VS Code

This repository recommends **JohnnyMorganz.luau-lsp** (VS Code extensions panel). Open the repository root as your workspace, install the recommended extension, and reopen a `.luau` package file. The checked-in `.vscode/settings.json` selects the **standard** (non-Roblox) platform, disables Rojo sourcemaps, and loads `types/bloxgloom.d.luau` as `@bloxgloom`. These workspace settings affect only editors that use them; they do not alter Bloxgloom's runtime or require Rojo. Other editors can point luau-lsp at the same definition file.

The definition file supplies **type aliases and nominal handle types** for host callbacks, plus the actual runtime `log` global. The aliases are editor-only; see [runtime tools](RUNTIME-TOOLS.md) for logging field validation and supported libraries. Annotate the host-supplied callback arguments in your modules, for example:

```luau
return function(host: BloxStartupHost)
    host.register_generator("demo:terrain", 1, "demo:terrain")
end
```

The definition file uses `---` documentation comments directly above types,
fields and methods. Hover a typed field such as `entity.id` or a method such
as `context.inventory` to see its meaning, units and indexing conventions.
These comments are supported by [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp#supported-features)
and were checked through actual hover requests with the installed language server.
Use **Luau: Reload Language Server** after changing global definitions.

Entity IDs, revisions and ticks are immutable host-created values, with distinct
editor types (`BloxEntityId`, `BloxRevision`, `BloxTick`). They preserve the full
Rust value without exposing numeric halves. Compare IDs/revisions with `==`, use
IDs as table keys, and use `tostring` for exact diagnostic labels. Passing a
revision where an entity ID is expected is a type error and a runtime error.
The handles grant no permissions: server ownership and transaction checks apply.

```luau
-- An entity view supplies its identity directly.
local current = context.entity(entity.id)
context.update_entity(entity.id, newState)

-- Compare exact simulation time without floating-point reconstruction.
if message.produced_tick:before(context.tick) then
    local elapsed = context.tick:elapsed_since(message.produced_tick)
end
```

Host callback services use **dot calls**, while handle helpers use **colon
calls**. Tick intervals must be nonnegative and no greater than `2^53`; larger
intervals error rather than silently rounding. Revision tokens support
`:is_initial()` for the zero version (including anchored motion revisions).
Generation and gameplay random helpers accept an optional small integer salt or
sequence and return a deterministic number in `[0, 1)`; the host consumes its
exact world seed internally. Startup component fingerprints use fixed-width
hexadecimal strings. See [Luau packages](LUAU-PACKAGES.md) for the runtime contract.

For the other callbacks, use `BloxGenerationContext`, `BloxGameplayContext` with `BloxActionEvent` (only for `ActionRequested`), `BloxOwnerContext`, `BloxClientStartupHost`, or `BloxUiInput`. A UI handler can annotate its return value as `{ BloxUiCommand }`. Owner planners return `(binary_state: string, delay_ticks: number)`; their readonly `inbox` carries `BloxOwnerIntentDelivery` values. Declarations may request radius-one chunk reads and same-system durable intent delivery, but the editor types do not prove authority or bounds. All methods on these callback tables are **dot calls**, not colon calls. Namespaced keys, revision ranges, capabilities in `package.txt`, state byte limits, ownership, and host budgets are still checked by Bloxgloom, not proven by these types. Use `BloxGameplayEvent` for a general `register_handler` callback and narrow on `event.kind`; the union includes block removal/placement, neighbor changes, entity ticks and pickup requests. `BloxActionEvent` describes only registered action requests. Startup types also include creature and machine declarations and optional storage footprint/screen groups.

The UI fixtures use strict checking and explicit input/output types:

```luau
--!strict
return function(input: BloxUiInput): {BloxUiCommand}
    if input.event == "uitarget:light" then
        return {{op = "action", key = "uitarget:light"}}
    end
    return {}
end
```

This gives completion for `input.event`, the other input fields and the returned
command shapes. Action commands may include optional binary `arguments`.
Replica presentation handlers instead use `BloxReplicaInput` and return
`{BloxReplicaCommand}`; these expose public entities, entered/left identities,
and the permitted visual commands. See the typed Prism and Phase 4 client
fixtures for examples. The host still validates ownership, sizes and value
ranges at runtime; annotations are erased when Luau compiles the source.

The host's `import("package:module")` resolves **manifest module identities** and direct dependencies, not filesystem paths or Luau `require` aliases. luau-lsp does not automatically model that loader or its side/authority rules; imports may show an unknown-global diagnostic and imported exports will not be inferred. Do not change scripts to `require` merely to appease the editor. This configuration does not declare a fake global `import`, enable Rojo, or claim Roblox APIs exist. A downloaded client startup module may import visible client/shared sources and initialize its own UI text/state, not world/inventory state. The client UI event module can request one package-owned registered item/empty/block/entity gameplay action from the current selection or ray hit, with optional binary arguments; only the server authorizes its result.

## Validate

1. In VS Code, inspect the language mode of an open `.luau` file (Luau), check completions after a typed callback parameter, and confirm that a misspelled callback method receives a diagnostic. Check **Luau Language Server** output for definition-file loading errors. Restart with **Luau: Reload Language Server** after editing the definitions.
2. If `luau-lsp` is installed separately, run `luau-lsp analyze --platform=standard --definitions=@bloxgloom=types/bloxgloom.d.luau path/to/your/file.luau` from the repository root. The extension ships a server binary on some platforms, but does not put it on `PATH`. Analyze a typed callback with no custom imports for a baseline; custom `import` diagnostics are an editor limitation, not a package loader result.
3. Run Bloxgloom's actual package startup/client UI paths to check runtime behavior; editor types do not validate the package manifest, source identity or gameplay authority.

Setting names and the `definitionFiles` object shape were checked against the extension's [current source manifest](https://github.com/JohnnyMorganz/luau-lsp/blob/main/editors/code/package.json); `.luaurc`'s [`globals` and `aliases` schema](https://github.com/JohnnyMorganz/luau-lsp/blob/main/editors/code/schemas/luaurc.json) does not provide typed host callback parameters or Bloxgloom module resolution, so no `.luaurc` is added here. Luau definition-file syntax is marked unstable by the extension.
