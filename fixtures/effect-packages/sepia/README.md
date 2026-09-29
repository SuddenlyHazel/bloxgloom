# Scene-color effect example

Use the parent `effect-packages` directory as the server's package root, or run:

    cargo run -- effect-preview /path/to/sepia.png

`sepia` is a format-2 package with a WGSL `shader` asset and a strict JSON
`effect` descriptor. These use new asset tags 6 and 7 in the existing canonical
v4 ClientBundle framing. Older clients reject these tags; existing packages,
save files and wire framing are unchanged.

This fixture uses the compatible version-1 contract, with one final
`scene_color` effect at order 0. [Version 2](../../../docs/modding/AUTHORED-VISUALS.md)
adds composable passes, typed parameters and declared inputs/outputs.
Duplicate slot ownership, orphan assets, unknown descriptor fields, other
stages/orders and foreign shader references fail preparation. The renderer
draws one fullscreen triangle after world geometry, before bloom/display
mapping. UI and target outlines are unaffected. It owns formats, attachments,
pass scheduling and draw counts; packages cannot submit GPU work.

The only entrypoint is a fragment `fs_main` taking builtin position `vec4f`
and returning location 0 `vec4f`. Required group-0 bindings are:

* 0: linear HDR scene `texture_2d<f32>`
* 1: renderer-owned filtering clamp `sampler`
* 2: uniform `vec4f`: elapsed local seconds, reserved zero, width, height

Time is presentation-only and restarts at GPU preparation. Resize preserves
the pipeline and refreshes dimensions and attachments. A renderer-owned RGBA16F
attachment is allocated within the graph resolution/memory budget. No read/write attachment aliasing occurs.

Limits: 16 KiB WGSL, 1 KiB descriptor, 64 IR types, 128 global expressions,
512 fragment expressions, 32 locals, 128 straight-line statements, three fixed
resources, one pipeline/pass. No helper functions, aggregate types, loops,
nested control flow, compute, overrides, storage resources, atomics or discard.
Existing package count and aggregate byte limits also apply.

Verified bundle decode runs Naga preparation on a scoped worker; GPU compilation
runs on a scoped worker with a validation error scope and package-attributed
errors. Results publish only after success. The live join shell polls GPU preparation while displaying progress; all
package pipelines must succeed before the session becomes live. Failure returns
to the retry screen with the resource context. Live reload remains deferred.
