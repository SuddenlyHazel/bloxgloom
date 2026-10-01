# Modular hair socket contract

Appearance/material IDs are append-only: 0 is no attachment, and IDs 1–13 select
all thirteen authored hairstyles. Material 0 always means the body.
`HAIR_PNGS[id - 1]` selects the matching 32 × 32 nearest-filtered texture.
Every material occupies a contiguous triangle-index range, recorded in
`hair_sockets.json`, so the renderer draws only the body and selected hair.

| ID | Style | Vertices / indices | Index range |
|---|---|---:|---:|
| 0 | body | 216 | 0–216 |
| 1 | tousled crop | 360 | 216–576 |
| 2 | side-swept undercut | 504 | 576–1080 |
| 3 | space buns | 792 | 1080–1872 |
| 4 | curly bob | 2664 | 1872–4536 |
| 5 | curly pigtails | 1764 | 4536–6300 |
| 6 | sidepart bob | 360 | 6300–6660 |
| 7 | compact braid | 432 | 6660–7092 |
| 8 | long loose curls | 2772 | 7092–9864 |
| 9 | long curly ponytail | 1980 | 9864–11844 |
| 10 | half-up curly cascade | 2412 | 11844–14256 |
| 11 | rounded afro | 4248 | 14256–18504 |
| 12 | twin braids | 3168 | 18504–21672 |
| 13 | curly mohawk | 1008 | 21672–22680 |

Ranges are start-inclusive/end-exclusive. Total storage is 22,680 vertices and
indices, or 7,560 triangles. Total validation budgets are 32,768 vertices and
98,304 indices; additional per-style vertex limits and head-local bounds are
enforced by both converter and native validation. These storage totals include
all attachments, not the geometry drawn for one selected style. Shared body and rig
live in `character.json`, original clips in `clip_*.json`; each hair has a bounded
`.mesh` containing
exact float32 vertex attributes and local triangle indices. Loading these parts
reconstructs the same native geometry and global material ranges above.

## Attachment and preservation

All styles attach rigidly to head joint 1 at identity local TRS. Source coordinates
are meters, +Y up and −Z forward; the native transform applies the existing
uniform 0.9 scale and 180-degree Y rotation once. Geometry, normals, UVs and
embedded PNG bytes retain the authored values. Existing body/face texels, seven
joints and five clips are unchanged. There are no new bones, hair physics,
independent braid/pigtail motion, corrective deformation or hidden clip edits.
Intentional head/scalp overlap hides seams and is excluded from body clearance.

The runtime adds no independent head-look rotation or corrective angle clamp.
Hair follows only the existing authored animation and production idle/walk blend.
The restrictions below must be revisited before adding another pose layer.

## Source-backed original-clip evidence

`hair_compatibility.json` combines the original, curly, variety and bold source
kits' validation reports. It records each report SHA-256, each exact imported hair
GLB SHA-256, and the common texture-independent body/rig/animation fingerprint.
All four source base GLBs have that same fingerprint, matching the native input.
Per-style preservation checks, all five clip results, per-body-part minima,
intentional scalp overlaps and head-stress failures remain in the report.

All thirteen styles pass the five unchanged source clips: walk, idle, crouch,
tool_use_left and tool_use_right. Each clip was checked at every authored key and
uniform 1 ms samples with complete hierarchical transforms and quaternion SLERP.
Each hair cuboid is compared to the torso, both arms and both legs using a
normalized 15-axis OBB separating-axis test. A positive margin is a conservative
separation lower bound, not exact Euclidean surface distance. This is dense
sampled evidence, not a continuous-motion guarantee.

The minimum across the five source clips for each style is:

| ID | Style | Tightest clip | Source / native minimum (mm) |
|---|---|---|---:|
| 1 | tousled crop | tool_use_left | 73.927 / 66.534 |
| 2 | side-swept undercut | tool_use_left | 21.287 / 19.159 |
| 3 | space buns | tool_use_left | 113.071 / 101.763 |
| 4 | curly bob | tool_use_right | 6.290 / 5.661 |
| 5 | curly pigtails | tool_use_right | 72.946 / 65.651 |
| 6 | sidepart bob | tool_use_left | 13.623 / 12.261 |
| 7 | compact braid | tool_use_left | 28.784 / 25.905 |
| 8 | long loose curls | tool_use_left | 59.884 / 53.895 |
| 9 | long curly ponytail | tool_use_left | 21.741 / 19.567 |
| 10 | half-up curly cascade | tool_use_left | 70.032 / 63.029 |
| 11 | rounded afro | tool_use_left | 44.743 / 40.269 |
| 12 | twin braids | walk | 6.141 / 5.526 |
| 13 | curly mohawk | tool_use_left | 177.781 / 160.003 |

Native values here are the source margins multiplied by 0.9. The tightest source
original-clip case is **twin braids in walk, about 5.526 mm at native scale**.
The curly bob also has a small margin, about 5.661 mm in right tool use.
New equipment, body proportions or pose layers need new clearance checks.

## Additional head-look limitations

The wide stress grid adds yaw ±35° and pitch ±15°, in 1° increments, at only
neutral and held crouch: 2,201 angle combinations per base pose. It is not
composed with all animation times. Ten styles pass both base poses; these three
have explicit collisions:

| Style | Neutral worst margin (source mm) | Held crouch worst margin (source mm) |
|---|---:|---:|
| long loose curls | −43.376 | −103.567 |
| half-up curly cascade | −7.965 | −60.935 |
| twin braids | −158.858 | −125.934 |

The three curly-kit styles pass a smaller yaw ±20° / pitch ±5° grid at neutral
and held crouch (451 angle combinations per pose). Its results are retained in
`additional_head_look_evidence.long_hair_reduced_grid`. This narrower grid is
still not a motion-wide supported envelope and adds no runtime permission.

Twin braids also fail a smaller composed-animation test: yaw −10°, −5°, 0°, 5°,
10° combined with pitch −5°, 0°, 5°, at 100 Hz plus every clip key. Walk and
crouch intersect (worst source margins about −110.424 mm and −31.745 mm);
idle and both tool clips pass that finite grid. The exact report is retained in
`additional_head_look_evidence.twin_braid_composed_grid`. Keep independent head
look disabled for twin braids until a complete runtime motion set is validated.
The bold kit's overall stress-suite failure is preserved, not relabeled all-pass.

## Native regressions

Adjacent Rust attachment tests sample all five clips at 121 uniform times plus
every authored key, checking finite positions, rigid head attachment and scaled
normals. These sampling checks do not by themselves test intersections.

`tests/clearance.rs` additionally reconstructs every complete cuboid from its
face normals and eight corners, checks exact counts for all thirteen styles, and
uses normalized 15-axis SAT against the five torso/limb boxes through the actual
native samplers:

- All five original clips at 121 uniform times including both endpoints, plus
  every authored key, with duplicate times removed
- 1,377 production idle/walk blends: 9 idle phases × 17 walk phases × 9 weights,
  including both blend endpoints; looping phase endpoints are duplicates of zero

The measured run passes all three clearance tests. The five original-clip grids
contain **872 poses** in total; every style stays clear on those poses and the
**1,377 blend poses**. The tightest measured native margins are:

| Native test grid | Poses | Tightest style | Minimum native SAT margin (mm) |
|---|---:|---|---:|
| walk | 161 | twin braids | 5.526 |
| idle | 241 | long loose curls | 60.920 |
| crouch | 148 | twin braids | 25.931 |
| tool_use_left | 161 | curly bob | 11.833 |
| tool_use_right | 161 | curly bob | 5.661 |
| idle/walk blends | 1377 | twin braids | 6.721 |

All thirteen per-style measured minima for each grid are retained in
`native_sampled_regression.measured_results`. These native measurements are
already scaled; do not multiply them by 0.9 again. The coarser native clip grids
can miss a tighter intermediate source-report sample, so retain both evidence
sets and their sampling limits.

The penetration tolerance is 0.00001 native meters. These are finite regression
grids, not proof for continuous motion, every blend weight/phase or added head
look. The original 1 ms source reports remain the denser clip evidence.
Python tests separately protect deterministic conversion, exact source
vertices/UVs/normals, original PNG bytes, stable IDs, socket bounds, contiguous
single-material ranges and source-evidence hashes.
