# Luau authoring in VS Code

This repository recommends **JohnnyMorganz.luau-lsp** (VS Code extensions panel). Open the repository root as your workspace, install the recommended extension, and reopen a `.luau` package file. The checked-in `.vscode/settings.json` selects the **standard** (non-Roblox) platform, disables Rojo sourcemaps, and loads `types/bloxgloom.d.luau` as `@bloxgloom`. These workspace settings affect only editors that use them; they do not alter Bloxgloom's runtime or require Rojo. Other editors can point luau-lsp at the same definition file.

The definition file exports **type aliases**, not runtime globals. Annotate the host-supplied callback arguments in your modules, for example:

```luau
return function(host: BloxStartupHost)
    host.register_generator("demo:terrain", 1, "demo:terrain")
end
```

The definition file uses `---` documentation comments directly above types,
fields and methods. Hover a typed field such as `entity.id_lo` or a method such
as `context.inventory` to see its meaning, units and indexing conventions.
These comments are supported by [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp#supported-features)
and were checked through actual hover requests with the installed language server.
Use **Luau: Reload Language Server** after changing global definitions.

For example, `id_lo` and `id_hi` are the low and high unsigned 32-bit words of
one 64-bit entity identity, each in `0..4294967295`. Keep both words together
and pass them unchanged to entity services. Luau numbers cannot represent every
64-bit integer exactly, so reconstructing an ID as `id_hi * 2^32 + id_lo` can
silently identify the wrong entity. Tick and revision pairs use the same word
encoding but represent simulation time and state versions respectively.

For the other callbacks, use `BloxGenerationContext`, `BloxGameplayContext` with `BloxActionEvent` (only for `ActionRequested`), `BloxOwnerContext`, `BloxClientStartupHost`, or `BloxUiInput`. A UI handler can annotate its return value as `{ BloxUiCommand }`. Owner planners return `(binary_state: string, delay_ticks: number)`; their readonly `inbox` carries `BloxOwnerIntentDelivery` values. Declarations may request radius-one chunk reads and same-system durable intent delivery, but the editor types do not prove authority or bounds. All methods on these callback tables are **dot calls**, not colon calls. Namespaced keys, revision ranges, capabilities in `package.txt`, state byte limits, ownership, and host budgets are still checked by Bloxgloom, not proven by these types. The `BloxActionEvent` alias does not describe other `register_handler` event shapes; consult the host binding for those rather than treating them as action events.

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
