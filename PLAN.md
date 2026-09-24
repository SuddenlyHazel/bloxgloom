# Bloxgloom interface plan

Status: implemented and headlessly validated. This replaces the original world-architecture plan; the authoritative server, terrain, persistence, chunk renderer, and movement were already in the repository when this phase began. The live 60 FPS target still needs validation on a visible desktop during chunk streaming; headless measurements cannot establish presented gameplay FPS.

## Goal

Make the current creative building loop usable: a precise crosshair and block target, a visible HUD and hotbar, an inventory for choosing blocks, and an Escape menu with settings and exit. Keep the world responsive at the existing 1280×720, 60 FPS target on an M1-class integrated GPU.

This phase is **creative mode**. Blocks have no quantities: the inventory is a catalog of placeable block types and the hotbar holds nine chosen types. That matches the current free-flight controls and avoids implying a survival economy that the server does not implement. Item stacks, crafting, health, and survival rules are separate future work.

## Baseline when this plan was written

- `src/client.rs` captures the mouse, handles flight and clicks, and sends an edit with block ID `2` for every placement. Escape only releases the cursor. Its aimed-block search samples along a ray in 0.1-block steps.
- `src/render.rs` renders world chunks but has no screen-space UI pass or target outline.
- `src/server.rs` validates edit reach and block IDs. `src/protocol.rs` already carries block edits and a server-clamped view-distance setting.

## Interaction contract

| State | Mouse | Keys | Escape |
| --- | --- | --- | --- |
| Playing | Captured; left removes target, right places selected block against target | WASD/Space/Shift move; wheel or 1–9 selects hotbar; E opens inventory | Opens pause menu and releases cursor |
| Inventory | Free; select a block from the catalog and assign it to a hotbar slot | 1–9 selects destination slot; E closes inventory | Closes inventory and returns to play |
| Paused | Free; Resume, Settings, and Exit buttons | No movement or edits | Resumes play |
| Settings | Free; edit local settings | Keyboard navigation and value controls | Returns to pause menu |

Losing window focus releases the cursor and pauses input. Closing a menu attempts to recapture it; if the OS requires a click, show a short “Click to capture mouse” hint. Menus stop local movement and edits but do not pause the authoritative world or other players. “Exit” closes the client; in the one-command local game, the in-process server ends with it, while a dedicated server keeps running.

## UI and game-state design

- Add a small `UiState` state machine that owns the active screen, focused control, selected hotbar slot, and mouse-capture intent. Route each input event through it before game controls so a click on a menu can never edit a block.
- Replace the sampled aimed-block search with an exact voxel-grid raycast. One result supplies the center crosshair’s target feedback, a thin world-space target outline, and the left/right-click edit coordinates. Use the same reach as the server or a stricter client limit; the server remains authoritative.
- Render a screen-space UI pass after the world using the existing `wgpu` device and frame. Draw a centered crosshair, nine-slot hotbar, selected block label, menu panels, and concise status text. Cache glyphs and static geometry; scale from physical window size and a user-configurable UI scale. Keep UI data separate from chunk meshes and uploads.
- Keep the default HUD honest: crosshair, hotbar, selected block, and a brief edit-rejection or connection message. An optional F3-style debug overlay can show coordinates, FPS/frame percentiles, chunk counts, and latency. Do not draw health, hunger, or stack counts until those systems exist.
- Use the existing grass, dirt, and stone block IDs in the first inventory catalog. A selected hotbar block replaces the current hard-coded dirt placement; the server still validates the ID and reach. Keep the hotbar mapping and selection in local settings so they survive restart. The server need not trust or persist creative UI layout.
- Settings in this phase: mouse sensitivity, field of view, view distance, UI scale, and fullscreen/windowed mode. Apply changes immediately, persist them in a versioned local config, and clamp invalid values. Send view-distance changes through the existing `SetView` request, then add a protocol acknowledgment for the server-enforced radius; the current protocol has no response, so the menu cannot truthfully display the effective value without that addition.

## Performance and correctness

- Keep menu and HUD work bounded: no per-frame font rasterization, no per-block UI draw calls, and no network or disk I/O on the render thread. Measure UI CPU time and GPU frame time during chunk streaming; retain the 60 FPS target rather than assuming a static menu is cheap.
- Preserve input invariants across focus changes and transitions: no stuck movement keys, hidden cursor, accidental world edit, or movement command while a modal screen is open.
- Test the voxel raycast on negative coordinates, chunk boundaries, and face selection; test screen transitions and slot selection as pure logic. Test that placement sends the selected block and that the server still rejects invalid edits. Add a config round-trip test because user settings must survive restart.
- Extend the existing headless GPU preview to capture Playing, Inventory, Pause, and Settings screens. Inspect images at 1280×720 and a smaller window size for readability, alignment, clipping, and crosshair visibility. Do not rely on an unviewable desktop window for visual QA.

## Delivery order

1. Extract input routing and the exact block raycast. Add the screen-state transitions and focused tests. World controls work only in Playing.
2. Add the UI render pass, crosshair, target outline, hotbar, and minimal HUD. Verify with headless screenshots and frame-time measurements.
3. Add the creative inventory catalog and hotbar assignment. Place the selected block; verify two clients can see each other’s edits through the existing authoritative protocol.
4. Add Pause and Settings screens, local settings persistence, focus handling, and Exit. Verify menu navigation, resize behavior, and clean shutdown.

Done means the default `cargo run` supports the full interaction loop, the dedicated-client path behaves the same way, useful tests pass, the menu/HUD previews have been visually inspected, and streaming plus UI has been measured against the 60 FPS target. Any shortfall is recorded with the hardware and scene used rather than hidden behind an average FPS number.

## Validation record

- The interaction loop, settings persistence, view-distance acknowledgement, exact raycast, server edit propagation, compact UI hit targets, material mapping/mipmaps and seam-free tiling, and sky camera basis are covered by 41 passing tests.
- Headless GPU previews of all four screens were inspected at 1280×720, 640×360, and 640×360 with 2× requested UI scale. Additional views facing toward and away from the sun verify that it is world-anchored. Small windows fit the UI and label the requested scale as fitted when capped. The game window was not opened because this environment cannot inspect a visible desktop window.
- On this MacBook Pro, 300 warmed 1280×720 Settings-plus-debug frames measured UI CPU preparation at 0.209 µs median / 0.292 µs p95 with cached geometry, and 0.128 ms median / 0.153 ms p95 when rebuilt. This excludes GPU rendering and chunk streaming.
- With generated material assets and a world-space sky shader, `cargo run --release -- perf 300 6` measured a 1280×720 headless radius-6 scene on an Apple M1 Pro / Metal: 507 requested chunks, 440 nonempty meshes uploaded through the production queue limits, 236 visible chunks, and 77,822 visible triangles. Across 111 upload-ramp plus 300 steady frames, CPU submit-side p95 was 0.434 ms overall (0.568 ms during upload), and GPU render-pass p95 was 0.230 ms overall (0.256 ms during upload), well below the 16.67 ms frame budget for this scene. Meshing was precomputed, GPU timestamps exclude upload copies, and there was no present, vsync, or per-frame GPU wait; these numbers are not live gameplay FPS.
- Full-frame timing during visible live chunk streaming remains unverified. The client logs FPS and frame-time p95/p99 every five seconds for that follow-up on a visible desktop.
