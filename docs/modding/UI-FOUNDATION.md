# Phase 6 UI foundation

egui 0.36 is the production UI foundation on the existing wgpu/winit surface.
The live renderer paints the playing HUD, inventory and container screens,
menus, join flow, and verified package documents through one egui pass. Slot
icons and the crosshair use small custom egui painters. The previous GPU UI
renderer remains in headless legacy previews and benchmarks only.

Verified package documents retain their bounded JSON widget model and
package-owned styles, fonts and images. Rust maps panels, labels, images,
buttons and inputs to egui widgets with scrolling and wrapping. Packaged font
bytes and the verified image atlas are installed in the session's egui context.
Document text and visibility can change through bounded local or replica
callbacks. Text input uses egui's selection, focus, clipboard and IME path;
the retained values and callback results are capped at 128 UTF-8 bytes.
Version-2 documents add dynamic descendants, scrolling/table containers,
checkboxes, sliders, selects and multiline editing. Stable IDs retain edit/focus
state; Lua replies are validated atomically. See [dynamic UI/input](DYNAMIC-UI.md)
for the current authoring contract. Direct egui access and animation authoring
remain outside this surface.

Luau executes on the bounded presentation worker, never inside the draw pass.
It may submit one package-owned action key and up to 130 argument bytes. The
client composes item, block or currently aimed entity identity from its
streamed state. The server still checks reach, target identity, inventory,
permissions and durable effects. Receipts provide the authoritative result.

The earlier Taffy layout and bitmap renderer remain available to old headless
preview fixtures. They are no longer used to paint production screens. The
document format is intentionally bounded and widget based; it does not accept
HTML/CSS or arbitrary unverified controls. Accessibility beyond egui's
keyboard focus and platform text handling remains a future refinement.
