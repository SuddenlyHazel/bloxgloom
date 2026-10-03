# Authored materials and effects

Format-2 packages can publish versioned WGSL hooks, typed presentation parameters,
and a declared scene-color graph. The renderer owns geometry, projection, GPU
bindings, attachments, and command submission. These resources never grant world,
inventory, entity, or save authority. Imported models and live reload are deferred.

The [Prism fixture](../../fixtures/visual-packages/prism/README.md) combines the
interfaces below. The original Jade material and Sepia effect descriptors still
work with their version-1 contracts.

## Material contract 2

Declare an `asset material surface assets/materials/surface.json` and a distinct
`asset material-shader surface_shader assets/shaders/surface.wgsl` in `package.txt`.
Asset keys share one namespace within a package, across all asset kinds.

```json
{
  "version": 2,
  "shader": "surface_shader",
  "targets": ["prism:tile", "bloxgloom:stone"],
  "textures": ["prism:tile", "bloxgloom:stone"],
  "vertex_offset": 0.04,
  "parameters": [
    {"name":"tint","kind":"color","default":[0.2,0.8,1,1]}
  ]
}
```

Targets and texture inputs are registered catalog texture keys belonging to the
package or `bloxgloom`. They resolve against the negotiated session catalog, so
saved numeric IDs do not become shader indices in package source. A target can
have one material owner; conflicting packages fail preparation. A material can
select up to eight target layers and sample up to four texture inputs. A bundle
can contain sixteen materials. Shader source is limited to 8 KiB per asset and
version-2 descriptors to 4 KiB.

### Package-owned environment lighting

One version-2 material in the bundle may include a global lighting selection:

```json
"environment_lighting": {
  "sun_intensity": 0.8,
  "ambient_intensity": 0.6,
  "environment_intensity": 1.2,
  "local_directionality": 1.0
}
```

All four fields are optional, default to 1, and must be finite numbers in 0–4.
Unknown fields, invalid values, and multiple selections fail verified bundle
preparation; conflicts identify both package-owned material resources. The
selection affects the whole scene, not only that material's texture targets.
Sun controls direct daylight; ambient controls indirect daylight; environment
controls reflected environment lighting. These do not change camera exposure or
voxel emission. Package scales multiply local client lighting scales, with the
result clamped to 0–4. An omitted selection preserves local settings.

A cozy or industrial theme can soften direct and ambient light, while a neon
theme can lower ambient light and retain stronger environment response. Author
emissive surfaces separately. This startup declaration is immutable for the
session and is not a dynamic `host.set_parameter` resource. Version 1 cannot
select environment lighting. The existing material asset codec carries the
selection, so older unmodified assets retain their exact bundle bytes.

### Bounded local-light shadows

One version-2 material per bundle may select `local_shadows` independently of
`environment_lighting`. This is a global, immutable session setting:

```json
"local_shadows": { "count": 2, "resolution": 256, "range": 16, "updates": 2 }
```

All fields are optional; the example gives their defaults. `count` is 0–4
(0 disables maps), `resolution` is 64–1024 texels per cube face, `range` is a
finite 2–32 blocks, and `updates` is 1–4 whole lights per frame. A whole light
refreshes all six faces together. Unknown fields, arrays, out-of-range values,
legacy material selections, and multiple owners fail verified preparation.
These are renderer-owned bounded resources; mods cannot supply GPU bindings or
allocate additional maps. Local client limits remain authoritative, including
an off setting. Omission preserves local settings.

Supported geometry is the production voxel mesh: opaque cube faces, cutout block
faces, crossed foliage, cube drops, and item sprite/cross drops. Instanced actor
cuboids keep their existing pose/tint presentation API; these material hooks do
not target them. Inventory/UI item icons retain their UI rendering path.

The renderer supplies these types:

```wgsl
struct BgVertex { position: vec3f, normal: vec3f, uv: vec2f };
struct BgSurface {
    albedo: vec4f, position: vec3f, normal: vec3f, uv: vec2f,
    light: vec3f, emission: vec3f
};
```

Supply `material_fragment(BgSurface) -> BgSurface`; optionally supply
`material_vertex(BgVertex) -> BgVertex`. Positions are world coordinates. Vertex
hooks run before host camera projection and directional lighting. Displacement
is clamped on each axis to the declared `vertex_offset` in `0..0.25` blocks;
visibility bounds include that allowance. Collision and selection remain the
server's original geometry. Surface hooks receive interpolated attributes, the
sampled RGBA texel, voxel/bounced light, and emission. Host fog, depth and the
cutout alpha test follow the hook; returning alpha below 0.5 discards a cutout.
Opaque surfaces remain opaque.

Stable helper functions are:

| Function | Value |
| --- | --- |
| `material_texture(uv: vec2f, index: u32) -> vec4f` | Declared input texture, linear color, mip level 0; indices clamp to the last input |
| `material_parameter(index: u32) -> vec4f` | Parameter in descriptor order, padded with zero; indices clamp to 7 |
| `material_time() -> f32` | Seconds since this session's material GPU preparation |
| `material_sun() -> vec3f` | Normalized world sun direction |

The fixed implementation uses camera group 0, catalog texture/light resources
in group 1, and one bounded presentation uniform in group 2. Package hooks use
the helpers; declaring bind groups or entry points is rejected. Helpers and
package functions/types are isolated across materials. The built-in material
uses identity vertex/surface hooks through the same voxel shader contract.

## Typed values from Luau

Each material or effect declares up to eight uniquely named parameters.
Supported kinds are `float`, `uint`, `bool`, `vec2`, `vec3`, `vec4`, and `color`.
Defaults must match the kind. Vectors use dense Lua tables; colors use four
components. All numbers must be finite. Optional `min`/`max` bound each component;
default bounds are `[0,1]` for colors and `[-1000000,1000000]` otherwise.
Unsigned integers must also be exact integers in `0..16777215`. On the GPU,
boolean values use 0/1, scalars use `.x`, and vectors use the corresponding
components of a `vec4f`. Slots beyond the declared parameters contain zeros.

A downloaded `client_startup` module can initialize an owned resource:

```lua
return function(host)
    host.set_parameter("prism:surface", "tint", {0.2, 0.8, 1.0, 1.0})
    host.set_parameter("prism:mix", "strength", 0.35)
    host.set_replica_handler("prism:replica")
end
```

An authored UI event or replica callback can return:

```lua
return {{op="parameter", resource="prism:mix", name="strength", value=0.5}}
```

The resource key names the **descriptor asset**, not its output or shader asset.
Only the executing package may update it. Replica parameter ownership follows
the handler package independently of the currently open UI document. UI updates
still require document ownership. The existing sixteen-command callback limit
applies, and the entire command batch is validated before any mutation. Pending
updates coalesce by resource and parameter. New connections construct new
schemas/defaults/startup values; old workers and values cannot update a replacement
session. Public replica bytes/windows are available to handlers; private server
state and shader/GPU operations are not.

## Effect contract 2

Declare an `asset effect mix assets/effects/mix.json` and a distinct
`asset shader mix_shader assets/shaders/mix.wgsl`:

```json
{
  "version": 2,
  "shader": "mix_shader",
  "inputs": ["prism:graded", "bloxgloom:scene_color"],
  "output": "prism:color",
  "after": ["prism:grade"],
  "order": 0,
  "scale": 1,
  "final": true,
  "parameters": [
    {"name":"strength","kind":"float","default":0.25,"min":0,"max":1}
  ]
}
```

Each pass has one or two inputs and one uniquely owned output. The reserved
`bloxgloom:scene_color` input is the unfiltered HDR world scene. Other inputs
name pass outputs in the same package or an exact direct dependency. `after`
names descriptor asset keys with the same visibility rule. Input dependencies
and `after` constraints take precedence over the optional `order` byte; ready
passes sort by ascending order, then descriptor key. Missing references,
cycles, duplicate/reserved outputs, disconnected passes and graphs without
exactly one final output fail preparation with resource context. The final
output feeds built-in bloom and display mapping, before UI drawing.

Supply `effect_fragment(uv: vec2f) -> vec4f` with normalized output UVs. Helpers:

| Function | Value |
| --- | --- |
| `effect_input(uv: vec2f, index: u32) -> vec4f` | Input 0 or 1 sampled in linear HDR; input 1 aliases input 0 when only one is declared |
| `effect_parameter(index: u32) -> vec4f` | Same typed packing as materials; indices clamp to 7 |
| `effect_time() -> f32` | Seconds since graph GPU preparation |
| `effect_size() -> vec2f` | Actual output attachment size |

Use only input indices 0 and 1; other nonzero indices select input 1. Return
linear HDR color. The renderer owns group 0 bindings 0/3 for input textures,
1 for the sampler and 2 for frame/parameter data. Packages cannot declare or
replace these bindings.

There are at most eight passes and two inputs per pass, with 16 KiB shader
source and 4 KiB descriptors. `scale` is a resolution divisor of 1, 2 or 4.
Intermediate attachments are `rgba16float`, at least 1×1, with a largest extent
of 2048 (or the device limit) and a combined 64 MiB budget. The renderer applies
an additional common resolution divisor when needed, preserving aspect ratio
up to pixel rounding. Bind groups are cached until resize; resizing preserves
pipelines and parameter values. No output is sampled while it is being written.

## Preparation and compatibility

Version-2 hooks allow finite scalar/vector/matrix operations, small structs,
helper calls and conditionals. Loops, recursion, arrays in authored hooks,
resource declarations, overrides and authored entry points are rejected.
Each function has at most 512 expressions and 32 locals, the module has at most
12 functions including the four helpers, and expanded helper-call work is
bounded to 4096. This also rejects small sources that hide exponential work.
The budget bounds code/work; it does not promise a particular GPU frame time.

CPU parsing/validation runs during bundle preparation. Material keys resolve
before content readiness. Package pipelines compile asynchronously while the
window displays joining progress. The candidate becomes live only after every
material and effect succeeds; errors preserve package/resource identity and
return to the retry screen. Cancelled candidates retire before another GPU
attempt can start. GPU compilation errors and device limits remain possible
after CPU validation; an invalid package is not silently replaced by built-ins.

Version-1 materials retain `custom_albedo(vec3f, vec2f, vec3f) -> vec3f` and their
single `texture` field. Version-1 effects retain their fixed scene fragment,
`stage="scene_color"`, and `order=0`. They declare the graph's final output, so
a legacy final effect cannot compose with another final effect. Bundle hashes
include descriptors/shaders; changing either changes the downloaded artifact
identity. Catalog/save identities still come from registered content keys.

The environment profile also accepts `local_directionality` (finite0–4, default1).
It scales the shared 65% directional local-light mixture; zero is isotropic,
values at/above1.539 are fully directional. The remaining component approximates
unresolved voxel scattering equally on terrain and all actors. The local config
counterpart is `lighting_local_directionality`. Emission tint currently derives
from normalized block reflectance, with neutral-white fallback for black emitters;
this is not an independently authored emission-color spectrum.
