# Dynamic recipe browser

Run with your usual operator profile and a new isolated save:

```sh
cargo run --release -- local-packages fixtures/recipe-browser/packages /tmp/bloxgloom-recipe-save
```

Give yourself stone and gravel through F4. Return to play and press **B** to
open the browser and populate its two server-backed recipes. The binding appears
in the normal binding settings and uses persistent local overrides. F6 opens the
document directly; edit a filter to populate rows if you opened it that way.

Search names, select a category, toggle selected-only, choose a row, and change
quantity. The callback replaces the table's descendants with stable row IDs.
Both the outer controls and recipe list scroll. Notes accept multiple lines and
stay local. Select matching component-free input in your hotbar before clicking
CRAFT. Crushing stone makes equal units of gravel; pressing gravel makes equal
units of stone. Quantities are 1–8. The server validates recipe, selected input,
quantity, and output capacity; every debit and output share one transaction.

UI state resets on reconnect. Inventory persists across reconnect and restart.
The UI never authorizes crafting, and rejected requests consume no input.

The browser reads the installed inventory, server world-clock sample, and own
action receipts through readonly typed replicas. It shows available plain input,
output capacity including the input slot freed by a complete craft, tagged
input excluded from recipes, and the latest
accepted/denied craft receipt. Unknown inventory/time appears as waiting. These
are local predictions: choose a matching hotbar stack and let the server validate
the operation. The quantity slider remains editable even above the predicted
maximum so a rejection can be reviewed without losing input.

Render the browser through the production egui preview. This dispatches the
declared binding through its actual callback before drawing the generated rows:

```sh
cargo run -- egui-preview /tmp/bloxgloom-recipe-ui fixtures/recipe-browser/packages
```

The initial previews show unknown server data. The `package-updated` previews
use a representative post-craft snapshot and a search filter to exercise the
ingredient, clock, component and receipt displays through the same UI worker.
This offline sample is separate from the real-server acceptance test.

Use a fresh save directory after editing package code during this prerelease.
The font is the existing UIDemo fixture font; its license is in `assets/fonts/`.
