# Bounded indirect-only scene contacts

Production terrain now evaluates validated moving-player and registered-creature footprints in its material
shader, replacing the old black alpha-blended contact decals. The existing floor
validator supplies only connected, resident opaque cube tops at the actor's support
height. Unknown cells, ledges, non-cube supports, emissive textures and voxel light
emitters are excluded. Receivers must match the top plane and upward geometric
normal, so actor sides and geometry below a ledge cannot receive the footprint.

Only sky diffuse is subtracted. Directional sunlight and its shadow visibility,
torch light, both bounce fields, material emission, specular, the dark-cave floor
and fog are unchanged. The voxel light samples are not modified and no second
static corner-AO factor is introduced. Overlapping dynamic footprints use maximum
occlusion rather than multiplying darkness. Contact strength fades with actor
height and receiver distance; no contact is visible beyond 28 blocks.

## Configuration and cost

`BLOXGLOOM_CONTACT_OCCLUSION=0` disables contacts; `1` is the default full calibrated
strength. Intermediate values scale strength; finite numbers clamp to 0..1 and
invalid/nonfinite values use the default. Read once when the renderer is created
(or sun-shadow quality is reconfigured). The setting works with sun shadows off.

At most 32 clipped floor cells are uploaded in a 1,552-byte contact uniform tail
shared with the existing sun-shadow binding. Selection keeps complete actor
footprints in caller distance/ID order; it never truncates a footprint at a cell
boundary. Budget membership can change as actor distance order changes, so crowded
scenes can still switch whole contacts at the cap. There is no temporal jitter or
stochastic selection. Far/side receivers return early; matching top fragments
perform at most 32 analytic footprint tests. There are no added textures,
render passes, per-vertex bytes or framebuffer-color copies. Custom material
packages retain the conservative disabled path.

This is bounded dynamic actor grounding, **not** full-scene SSAO or GTAO.
Registered mobile entities use their validated body half-width, so native meshes
and packaged GLB creatures share the same support contract without genre-specific
model names. The soft radius is bounded to 0.2–1.35 blocks, with up to 0.06 extra
airborne spread. Each footprint validates at most 25 floor cells. Body bob and
authored clip motion do not move the support anchor; authoritative airborne
height fades contact. Rigid moving entities/projectiles remain excluded because
their origins are body-centered rather than foot/support positions. Static environmental
occlusion still comes from voxel sky transport/corner sampling. Bounced light is
intentionally left alone, and foliage is not a contact caster. Hardware timing
and crowded-scene motion validation remain separate acceptance checks.

## Verification

`cargo test scene_contact -- --include-ignored` executes the production WGSL on a
GPU adapter and checks receiver bounds, ledge/side rejection, overlapping contacts,
distance and disabled fallback, unchanged cave floor/glow/direct light, finite
configuration and whole-footprint upload limits. Existing `contact_shadow` tests
cover resident-floor geometry and emissive metadata exclusions.

For a fixed visual comparison, run `outdoor-preview` twice with
`BLOXGLOOM_CONTACT_OCCLUSION=0` and `=1`; keep sun-shadow quality, exposure, TAA,
adapter and all other settings identical. Inspect the feet in
`02-canopy-character.png`. `outdoor-motion-preview` regenerates contacts from the
presented moving actors every frame. Software rendering checks correctness only.


## Creature fixture

`outdoor-creature-preview <directory>` and
`outdoor-creature-motion-preview <directory>` extend the normal outdoor views with
four independently tinted packaged GLB creatures. They register preview-only
content through the same catalog as mods; builtin IDs and world content do not
change. Use `BLOXGLOOM_CONTACT_OCCLUSION=0` / `=1` with one copied executable and
identical other settings for matched creature-grounding captures. The fixture
uses the existing authored-model test asset, not a new art pack.
