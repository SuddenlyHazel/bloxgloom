# Phase 6 UI foundation

Decision: use egui 0.36 as the shared Rust UI foundation. The merged F7 proof
renders a responsive inventory and machine interface through the existing
wgpu/winit surface and uses egui for buttons, text input, focus, selection,
clipboard, drop-downs and scrolling. Its inventory slots use custom painting
over egui interaction. Desktop and compact renders are in `EGUI-POC.md`; the
proof was visually reviewed, and the root tests, formatting and strict Clippy
passed before it was merged.

The earlier Taffy flex header and Taffy-backed package documents established
verified package resource and Luau event paths. They remain active until those
paths and every built-in screen are migrated. Taffy supplies geometry but not
text editing, focus, controls or rendering; the custom layer still limits
authored text to ASCII. Blitz was considered earlier, but the egui proof now
demonstrates the required embedding path in this game. Eguis immediate model
requires Rust-owned per-session widget state and stable IDs for authored
documents. Luau remains on the bounded presentation worker, and the server
continues to authorize all gameplay requests.

Phase 6 will replace the F7 proof with the normal UI runtime, map verified
documents/fonts/images to egui, support dynamic documents and robust text,
complete the authorized UI action bridge, and migrate built-in screens. Keep
the existing renderer only as a migration seam; remove the duplicate path
before marking the phase Done. Graphics and input behavior must be inspected
in the live release window or generated previews at desktop and compact sizes.
