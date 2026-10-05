# World generation improvement proposal

The biggest improvement would be to give each region a recognizable landscape identity: mountain ranges, broad valleys, plateaus, wetlands, coastlines, and distinctive underground routes. Tectonic provides a strong reference for building those from coordinated noise fields and splines.

This proposal covers terrain density functions, regional landforms, mountain detail, caves, underground rivers, surface rules, configuration, and placement, compared with Bloxgloom's generation pipeline. It records recommendations; the proposed systems have not been implemented or benchmarked. The recommended build order reflects dependencies and likely impact.

The review was conducted on October 5, 2026, against Bloxgloom revision `ebffdccf61ecc28c194f27b89abaa131733e7e8a` and Tectonic revision `34241bdb35acda67b5367d49f354c66c05e098e2`, checked out at `/Users/hazel/src/tectonic`. Tectonic source links below are pinned to that revision.

Bloxgloom already has useful foundations: deterministic generation, five biomes, caves, rivers, lakes, ponds, vegetation, constrained surface patches, and generation contributors. The main limitations are:

- Surface height comes from one additive formula centered around Y=21, with a relatively small mountain contribution. The declared maximum terrain height is 64.
- Rivers follow roughly parallel lanes, spaced 384 blocks apart, with a fixed water level of 16.
- Biomes use hard temperature and moisture thresholds, and surface materials depend mainly on biome and patch selection.
- Underground terrain uses one combined cave threshold.
- Trees share one broadleaf shape.

These are visible in [terrain generation](../src/world/terrain.rs), [hydrology](../src/world/terrain/hydrology.rs), and [height bounds](../src/world.rs).

Most of Tectonic's terrain design lives in JSON density-function graphs. It models landforms through noise, splines, and masks; "erosion" is a terrain-shaping parameter. Some capabilities, particularly aquifers, ore veins, and existing biome features, come from Minecraft's engine, so we would need our own implementations. Tectonic's checkout contains no block, foliage, or item textures: its only images are two `pack.png` icons. New Bloxgloom materials need their own textures.

For the overall shape of the world, make these changes:

1. **Create actual continents, oceans, and inland regions.**

   Our current continent field contributes only a small height adjustment. Give continentalness responsibility for land and ocean distribution, coastal shelves, inland elevation, and distance from the coast. This would make long journeys reveal meaningful geography. Tectonic separates [continental noise][continental-noise] from terrain shaping.

2. **Generate islands with a separate terrain system.**

   Use distinct island fields for archipelagos, isolated mountainous islands, and low coastal islands, constrained to ocean regions. Tectonic's [island noise][island-noise] combines multiple fields and gates their contribution using continentalness.

3. **Separate elevation, slope, and ruggedness.**

   A high plateau, a steep mountain, and a low rocky hill should have different controls. Tectonic separates terrain offset, factor, jaggedness, and roughness. Adopt equivalent concepts so increasing elevation does not automatically make every surface steep or noisy.

4. **Use splines to shape terrain deliberately.**

   Replace much of the additive height formula with authored curves over continentalness, erosion, and ridges. Curves can preserve wide valley floors, create plateau shoulders, and sharply increase elevation in mountain regions. Tectonic's [continental offset spline][continental-offset] is a particularly useful reference.

5. **Build mountain ranges with connected ridgelines.**

   Our mountains currently resemble raised noise patches. Combine broad range placement with finer ridge detail to create connected crests, saddles, subsidiary peaks, and valleys. Tectonic uses [mountain ridges][mountain-ridges] and [detail shifted by slope estimates][mountain-detail].

6. **Introduce regional weathering and erosion character.**

   Some ranges should be rounded and worn; others should have sharp ridges and broken faces. Use erosion to control broad landform shape, then localized weathering to affect mountain detail. Tectonic's [weathering function][mountain-weathering] demonstrates the latter. Physical erosion simulation could be a later experiment.

7. **Allow several landform styles within each climate.**

   A desert could contain dunes, rocky uplands, mesas, and broad basins. A temperate region could contain rolling plains, valleys, and plateaus. Tectonic blends regional families through [regional offset selection][regional-offset]. This creates variety without requiring dozens of biomes first.

8. **Add plateaus, mesas, terraces, and canyon profiles.**

   Shape flat upper surfaces, steep escarpments, and narrow or broad valley floors explicitly. Tectonic includes [plateau profiles][plateau] and separate single and double valley profiles. These would produce strong silhouettes and excellent building locations.

9. **Make desert dunes directional and asymmetric.**

   Our dunes are small noise undulations. Introduce dune fields with coherent orientation, curved crests, gentle windward slopes, and steeper lee faces. Tectonic's [dune function][dunes] uses shifted fields and nonlinear shaping to produce more structured forms.

10. **Add rare natural landmarks and selective 3D terrain.**

    Use regional masks for jungle pillars, isolated spires, dramatic headlands, overhangs, and occasional natural arches. Tectonic has [jungle pillar controls][continent-settings] and [region-dependent 3D roughness][terrain-roughness]. Reserve these features for suitable regions so they remain memorable and affordable to mesh.

Water, biome transitions, and surface treatment would make those landforms feel connected and believable:

11. **Make rivers part of the valley system.**

    Replace the visible parallel-lane pattern with channels derived from the same ridge and valley fields that shape surrounding terrain. River valleys should influence elevation, banks, vegetation, and nearby landforms together. Tectonic's ridge scale explicitly affects rivers and surrounding plateaus in its [continent settings][continent-settings].

12. **Add tributaries, confluences, and terrain-aware drainage.**

    This would extend beyond Tectonic's noise-based reference. A bounded regional drainage graph could connect streams to larger rivers, lakes, and oceans, with widths related to catchment size. It is a larger project, but would substantially improve exploration and geographical coherence.

13. **Let rivers pass through mountains underground.**

    Preserve a channel while creating a roof above it, rather than cutting every mountain into an open trench. Vary tunnel height, roof shape, and support pillars. Tectonic's [underground river composition][underground-rivers] is one of its most valuable ideas for Bloxgloom.

14. **Expand water landscapes beyond isolated basins.**

    Add wetlands, floodplains, irregular lake margins, coves, estuaries, and shallow coastal shelves. Tectonic's [wetland shaping][wetlands] and [ocean terrain][ocean-terrain] provide references. Keep water containment and level consistency explicit.

15. **Separate biome identity from continuous environmental fields.**

    Retain temperature, moisture, elevation, slope, and wetness as continuous values after selecting a biome. Use them to blend ground cover and vegetation across boundaries, and introduce foothills, alpine bands, riverbanks, and coastal variants. Tectonic shares climate and terrain parameters across its regional functions.

16. **Make surface materials respond to their surroundings.**

    Put exposed rock on steep faces, deeper soil on gentle terrain, sediment beside water, and snow according to temperature and altitude. Apply rules to exposed floors and ceilings when introducing overhangs. Tectonic's [surface rules][surface-rules] combine slope, water, biome, temperature, noise, and surface depth. Our existing surface patches can remain as local variation within these constraints.

17. **Give vegetation distinct regional shapes and distributions.**

    Add conifers, taller forest trees, sparse dry woodland, shrubs, reeds, and alpine ground cover. Vary density using moisture, slope, altitude, canopy conditions, and regional clustering. This is our own content work: Tectonic's vegetation field primarily helps drive Minecraft's climate and biome system. Our [tree placement](../src/world/terrain.rs) already provides deterministic anchors across chunk boundaries.

Underground generation needs its own recognizable geography:

18. **Create several cave families.**

    Combine large chambers, medium winding tunnels, narrow passages, and occasional ravines, each with separate frequency and size controls. Tectonic explicitly distinguishes cheese, spaghetti, noodle, and carver systems in its [cave settings][cave-settings]. This would replace our single dominant cave texture.

19. **Control caves by depth below the local surface.**

    Use a smooth cover-depth mask to protect ordinary surface terrain while permitting deliberately placed entrances, sinkholes, and exposed cliff caves. Tectonic's [depth cutoff][cave-depth] is a useful model. Entrance frequency should be a deliberate control.

20. **Preserve pillars, shelves, and floors inside large caves.**

    Compose solid features back into carved spaces to create recognizable chambers, bridges, ledges, and navigable routes. Tectonic's [cave composition][cave-composition] includes pillar constraints. Connectivity and usable walking space deserve explicit evaluation.

21. **Add underground water bodies and aquifer boundaries.**

    Generate flooded chambers, underground lakes, and wet and dry cave transitions with stable regional water levels. Tectonic enables Minecraft's aquifers, but the implementation is external to this repository. We would need a fluid occupancy model that avoids leaking water into every connected cave or creating abrupt chunk-edge water walls.

22. **Add rare deep lava tunnels and distinct underground decoration.**

    Tectonic shapes [lava tunnels relative to the world bottom][lava-tunnels], and places [glow lichen][glow-lichen], ice, and hanging lanterns around underground rivers. For Bloxgloom, localized fungi, roots, crystals, and mineral deposits could provide equivalent character while preserving dark sealed caves.

23. **Introduce geological layers and useful resource distributions.**

    Replace uniform underground stone with regional rock types, strata, sediment pockets, and clustered veins. Tectonic includes layered surface materials and adjusts ore placement when vertical bounds change. Its [height-stabilized placement][ore-placement] demonstrates how resource frequency can scale with available depth. A geological resource system would be our extension.

The engineering improvements below would let us tune and ship these features reliably:

24. **Create a frozen, configurable world-generation profile.**

    Expose continent size, ocean coverage, mountain height, ridge spacing, landform distribution, biome scale, and cave frequency independently. Add coherent presets such as Highlands, Archipelago, Gentle Valleys, and Frozen Frontier. Tectonic's [configuration][terrain-settings] and [presets][presets] show useful separation between controls.

25. **Split generation into focused stages with shared samples.**

    Use modules for environmental fields, landforms, density, hydrology, surfaces, caves, resources, and decoration. Compile configuration at startup and share immutable samplers. Start with clear Rust functions and spline definitions; add a general density-graph interpreter only if authoring needs justify it.

26. **Extend contributor APIs to describe the terrain actually being generated.**

    Contributors currently receive builtin terrain samples, and their writes compose in lexical key order. A contributor that changes terrain cannot automatically provide updated height or environmental samples to later decorators. Extend the contract deliberately so features can query the appropriate generation stage while preserving deterministic ordering and bounded execution. See [current composition](../src/world/generation.rs).

27. **Cache shared fields and measure generation cost independently.**

    Evaluate temperature, continentalness, regional masks, and other 2D fields once per column or bounded region. Use coarse interpolation for suitable 3D density fields, and skip irrelevant features early. Tectonic repeatedly uses `flat_cache`, `cache_2d`, `cache_once`, and `interpolated`. Measure chunk-generation latency separately from lighting, meshing, and rendering before selecting interpolation resolution.

28. **Treat distant representation as part of terrain design.**

    Mountain silhouettes, thin spires, river cuts, cave openings, and overhangs must survive LOD transitions. Our LOD system already preserves vertical spans, but wider cells use center sampling and can miss narrow features. Evaluate adaptive sampling or feature-aware summaries. Taller generation also affects [LOD coverage and error bounds](../src/world/lod.rs), [skylight bounds](../src/world/sky.rs), and spawn scanning.

29. **Persist the complete generation identity.**

    Include seed, generator revision, preset, normalized parameters, and relevant content identity in world metadata and cache keys. Chunk generation, point sampling, previews, and LOD must agree on the same terrain. Our [LOD identity](../src/world/lod.rs) already includes several of these inputs. For incompatible changes, increment the world folder version as the project guidance requires.

30. **Improve spawn selection and generation evaluation.**

    Select a safe location with useful resources, manageable slopes, and interesting nearby geography, then verify authoritative blocks. Our [spawn selection](../src/server/spawn.rs) currently searches at the origin. Add a fixed multi-seed review suite covering maps, cross-sections, ground-level views, cave connectivity, water containment, and LOD transitions. Seam tests should compare consistent generation across boundaries; intentional cliffs make a universal small adjacent height difference assertion unsuitable.

Two further improvements would build on the terrain system but require our own world and gameplay design:

31. **Place discoveries using landscape context.**

    Ruins on ridges, abandoned workings beside mineral deposits, camps near crossings, and hidden spaces beside underground rivers would make travel purposeful. Use slope, terrain fit, regional spacing, and access constraints when placing them. Tectonic supplies useful placement ideas, but does not provide a general authored discovery system.

32. **Connect terrain to traversal, ecology, and atmosphere.**

    Mountain passes, swimming, boats, climbing tools, wetland wildlife, sparse alpine vegetation, and region-sensitive ambient sound would make geography affect play. Water currently remains walk-through, so aquatic landscapes need traversal work alongside generation. These systems should consume shared environmental data; authoritative gameplay must continue to use server-owned world state.

Implement the work in this order:

| Order | Deliverable | Main payoff |
| --- | --- | --- |
| 1 | Shared environmental fields, splines, regional landforms, configurable profile | Recognizable regions and much stronger silhouettes |
| 2 | Connected mountain ridges, weathering, slope-aware surfaces, altitude bands | Mountains and valleys gain believable detail |
| 3 | Terrain-linked rivers, wetlands, coastlines, underground river passages | Landscapes connect into exploration routes |
| 4 | Multiple cave families, structural features, underground decoration | A varied underground world |
| 5 | Vegetation families, geological resources, contextual discoveries | Regional identity and reasons to explore |
| Throughout | Determinism, generation identity, LOD, bounds, visual and performance checks | Reliable streaming and consistent worlds |

The first milestone should be a broad valley between two connected mountain ranges, with a plateau on one side, exposed rock on steep slopes, and a river following the valley. That one scene would exercise the most valuable architectural changes and give us a concrete visual standard before expanding the feature set.

During implementation, preserve startup catalog registration and existing numeric content identities, server ownership of terrain and gameplay, worker-based generation and rendering preparation, and revision checks for stale jobs. New resources and discoveries must respect finite inventories and server-owned drops. Save-format incompatibilities should use a new world folder version; this proposal does not require world converters.

Validate code changes with `cargo test`, `cargo fmt --all -- --check`, and `cargo clippy --all-targets --all-features -- -D warnings`. Inspect generated previews or the release game window for visual changes. For rendering or meshing changes, compare `cargo run --release -- perf 300 6`, adding `bounced` when relevant; report scene setup, mesh size, CPU frame time, and GPU frame time separately. Measure generation latency and live streaming separately, because that benchmark excludes presentation and live gameplay. Use isolated temporary saves for tests and benchmarks, and exercise the real nonblocking-listener path if startup or server lifecycle changes.

[continental-noise]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/noise/raw_continents.json
[island-noise]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/noise/raw_islands.json
[continental-offset]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/terrain_spline/offset/continents.json
[mountain-ridges]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/mountain_ridges/ridges.json
[mountain-detail]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/mountain_ridges/shifteddetail.json
[mountain-weathering]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/mountain_ridges/weathering.json
[regional-offset]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/terrain_spline/offset/regions.json
[plateau]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/region/club/plateau_spline.json
[dunes]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/region/diamond/dune/final.json
[continent-settings]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/java/dev/worldgen/tectonic/config/ConfigState.java#L231
[terrain-roughness]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/terrain_spline/roughness.json
[underground-rivers]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/underground_river/total.json
[wetlands]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/region/diamond.json
[ocean-terrain]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/terrain_spline/ocean.json
[surface-rules]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/minecraft/worldgen/noise_settings/overworld.json
[cave-settings]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/java/dev/worldgen/tectonic/config/ConfigState.java#L344
[cave-depth]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/overlay.mod/data/tectonic/worldgen/density_function/__constants/cave/depth_cutoff.json
[cave-composition]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/caves.json
[lava-tunnels]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/density_function/lava_tunnel/total.json
[glow-lichen]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/resources/resourcepacks/tectonic/data/tectonic/worldgen/placed_feature/underground_river/lichen.json
[ore-placement]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/java/dev/worldgen/tectonic/worldgen/placementmodifier/HeightStabilizedCount.java#L48
[terrain-settings]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/java/dev/worldgen/tectonic/config/ConfigState.java#L200
[presets]: https://github.com/Apollounknowndev/tectonic/blob/34241bdb35acda67b5367d49f354c66c05e098e2/src/common/main/java/dev/worldgen/tectonic/config/ConfigPresets.java
