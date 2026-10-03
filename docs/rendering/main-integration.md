# Latest-main rendering integration (2026-10-03)

The rendering campaign at `913bc0f99ba0d4fd2dbf6ce544152bd21e974f87` is
reconciled with main `096d4ec0df94fe718fed51dcb22a0bfd57a04290`. Both parent
histories are retained; the rendering branch does not merge itself into main.

## Resolutions

- Preserve packaged player GLBs, named controls, baked clips, owner-only hiding
  and camera offsets alongside packed colored lighting and scene MRT outputs.
- Include the exact owner camera offset in submitted-pose history, and invalidate
  history on model, view-mode, visibility or material changes. Owner-hidden
  geometry is absent from motion output; world shadows stay complete/unshifted.
- Keep generic body-sized mobile contacts, including packaged-player contacts.
- Preserve moving-body and lighting declaration wrappers. Their independent
  version-50 allocations collided: lighting now uses version 52 outside authored
  creatures (51), moving physics (50) and model/player declarations (49).
  A combined real-package round-trip regression covers this entire stack.
- Keep main's cache/reload, behavior/save contracts, world/player APIs and Rapier
  changes. Use fresh `world-v27` / `world-v27-fixture` folders because the combined
  catalog differs from previous builds. Existing saves are neither deleted nor
  converted.

## Final-main integration

- Keep transactional player health, ordered respawn checkpoints, compact health
  HUD, audio buses/compression, and the 256px Rock030 material trial.
- Preserve both local-shadow and audio-mixer config parsing; exercise their
  nondefault values together in the persisted-settings round-trip regression.
- Derive local-shadow GPU alpha-cutout fixture indexing and UVs from the shared
  terrain texture-size constant rather than assuming 128px.
- Main and the earlier rendering merge independently selected `world-v26`.
  Advance this combined build to `world-v27` / `world-v27-fixture` so the
  different foliage/skylight fingerprint cannot collide with main saves.

## Earlier merge verification (3987308)

- `cargo test --workspace --all-features -- --test-threads=1`: 1,808 application
  and 60 host-API tests passed; zero failures; 12 intentionally ignored tests.
- Separately ran the three ignored GPU correctness tests (daylight, fog and
  scene contacts): all passed. Performance/pressure fixtures were not run.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo fmt --all -- --check` and whitespace/conflict checks passed.
- Fresh default gameplay and eight matched local-shadow off/on frames used the
  final production build. Inspected gameplay, third-person and local-shadow
  frames; packaged player/humanoid GPU captures cover named appearance changes,
  baked animation and owner-only geometry hiding. Their three targeted GPU
  tests passed again against the final test executable.
- Earlier production executable SHA-256:
  `53872c227fd8f2dede598804cc0b9e452b73ec4e4975173a5b2a483d59b85181`.

Captures used SwiftShader/Vulkan in this cloud environment. They validate
production rendering paths, not an interactive window, hardware frame cost or
hardware-specific TAA behavior. Existing wgpu warnings about non-invariant
motion outputs with Equal depth remain. TAA stays opt-in; live local shadows
retain count 2, resolution 256, range 16 and two updates per frame. Graphify
query/update could not run because the executable is unavailable.
