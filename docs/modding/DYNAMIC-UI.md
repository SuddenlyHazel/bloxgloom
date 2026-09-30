# Dynamic authored UI and input

Version-2 documents extend the verified JSON UI contract. Version-1 documents
keep their original widget kinds and bounds. Luau callbacks run on the existing
presentation worker; egui only draws the published model and emits local intents.
The runnable [recipe browser](../../fixtures/recipe-browser/README.md) demonstrates
search, filtering, selection, scrolling, controls, rebindings and real server
crafting receipts.

## Documents and controls

Set `version` to `2` and declare the existing `presentation` module binding.
All styles, images, fonts, modules, events and input-action identities remain
package-owned and are installed from verified package bytes before play.

| Kind | Fields and behavior |
| --- | --- |
| `panel` | Group children vertically or in wrapped rows according to its style |
| `scroll_panel` | A separately scrolling container, limited by style height |
| `table` | Child containers form rows; their children form cells |
| `label`, `image`, `button` | Existing text, static packaged image and event button |
| `input` | Single-line UTF-8 editing, at most 128 bytes |
| `multiline_input` | UTF-8 editing with LF newlines, at most 1024 bytes |
| `checkbox` | Boolean `checked` (default false); `text` is its label |
| `slider` | Finite `min`, `max`, `value`; optional positive `step` |
| `select` | 1..64 distinct `{key,label}` options and optional `selected` key |

Styles still require a nonzero `height`, even for containers; `width=0` adapts
to available width. Textual widgets require a declared font. Sliders require
`min < max`, a finite range, an in-range value and a declared step-aligned value.
Select keys are local identifiers; selection must name an option. Other control
characters, NaN/infinity and unsupported fields are rejected. Browser controls
use egui focus, keyboard navigation, clipboard and IME handling.

A version-2 document admits up to 256 nodes with depth at most 16. The verified
resource set admits up to eight documents, 1024 nodes and 32 KiB reserved text
when version-2 documents are present. JSON assets remain limited to 16 KiB each;
existing style/font/image/atlas and bundle limits remain in effect. Runtime trees
are limited to 256 nodes and 32 KiB of text, reserved control capacity and option
data. Empty editable controls reserve their full byte capacity, so later local
editing cannot exceed the aggregate budget.

## Callback inputs and atomic updates

Callbacks retain `sequence`, `event`, string `value`, explicit 128-byte `state`
and the text map `texts`. Widget events also include their full stable `node`
identity, `value_typed` for editable controls, and a readonly `values` map of all
current editable values. Checkbox values are booleans, slider values are numbers,
and input/select values are strings. The input and its maps are readonly.
Declared key events have no widget identity and carry `value="pressed"`.

```luau
return {
    {op="children", node="recipe:browser/recipes", nodes={
        {id="row_stone", kind="panel", style="recipe:row"},
        {id="choose_stone", parent=0, kind="button", style="recipe:button",
            text="Crush stone", event="recipe:select"},
    }},
    {op="value", node="recipe:browser/selected_only", value=true},
}
```

`children` replaces all descendants of a version-2 container. Its dense flat
array uses zero-based parent indices within that array; roots omit `parent` and
attach to the target. Empty arrays remove all descendants. Replacing the array
can create, delete and reorder widgets. IDs must be unique local identifiers;
they resolve to `package:document/id` and cannot collide with retained nodes.
Styles/images must already exist in the frozen package resources. No script
creates fonts, textures, styles or new gameplay definitions during play.

Editable values and visibility survive replacement when stable identity and
compatible kind/value contract survive. New declaration text updates labels;
`value` explicitly resets an editable value. Focus follows an interactive stable
identity and clears when that widget disappears or becomes hidden. Actual egui
intents carry the tree generation, so an old click/edit cannot target a new
widget that happens to reuse its array index. Player text updates locate current
identities, rather than old template positions.

Callbacks still return at most 16 commands. `text`, `visible`, `state`, visual
parameter updates and one semantic `action` remain available. `value` accepts a
boolean, finite number or string validated against the target control. Multiline
text updates allow LF within their 1024-byte limit. The whole reply is planned
and validated before publication; an invalid tree/value/ownership/action target
publishes no partial widget, state, parameter or gameplay request changes.
Exceptions and quota failures retain attributed diagnostics and disable the
handler until reset. One outstanding invocation is admitted; busy edits are
rejected visibly rather than silently queued.

Replica callbacks can update an active document owned by their package. They
receive document state, text and values only when they own that document.
They cannot request gameplay actions. Public replica observations remain the
existing bounded services; dynamic UI adds no private server-state access.

## Declared input actions

```json
"bindings": [
  {"key":"recipe:browser", "event":"recipe:open", "default":"B",
   "scope":"game", "open":true}
]
```

A document declares at most 16 bindings and the resource set at most 64 distinct
keys. `scope` is `game` (default), `ui` or `both`. Bindings operate for the current
document, queue its presentation callback, and can open the package screen after
successful admission. They never directly authorize a server command.

Defaults and overrides use physical letters; movement keys and configured native
controls are reserved. Persisted named bindings override defaults. Colliding
native/named assignments disable that default rather than stealing another
control. Repeats and busy presses do not dispatch again or fall through to a
server command. Modifiers, focused text entry and unrelated modal screens block
mod bindings. The local F4 bindings interface lists declared input actions and
saves overrides through the existing background config writer.

## Server authority and remaining scope

The recipe example requests only `recipe:craft` and bounded recipe/quantity
arguments. The client composes the selected slot and current inventory revision;
the server validates input identity/components, quantity and output capacity,
and commits input/output together. UI text is never evidence of acceptance:
host receipt chrome distinguishes pending, denied and applied requests.

UI trees and values reset on reconnect/document cycling; closing and reopening
retains the active document. This contract does not expose direct egui/HTML,
a general animation API, arbitrary GPU access, mouse/gamepad bindings or modifier
chords. Imported models, VM reuse, hot reload and save converters remain separate
work.
