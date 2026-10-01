# Farming package composition fixture

One package supplies 32 crops × four stages (128 blocks), 64 seeds/produce items
(192 total items including blocks), eight textures, 96 source modules, three
independent owner systems and two contributors. Crop and recipe modules define
registered content; the 22 calendar modules define season durations. They are
gameplay data rather than padding used to inflate the module count.

Run with a fresh isolated save:

```sh
cargo run --release -- local-packages fixtures/farming-scale/packages /tmp/farming-review-save
```

F4 `give farm:barley_seed 128` supplies the finite inventory. Select those seeds,
aim at stone, and use F6 **Plant barley**. It consumes one seed and replaces
stone with a seedling atomically. The demonstration bed at `x=2..5,y=81,z=0`
advances barley one stage when growth runs. Irrigation wakes growth every 100
server ticks, ahead of growth's own 1,000-tick deadline. Aim at ripe barley and
use **Harvest ripe barley** to replace it with stone and insert one produce;
a full inventory rejects both effects. Stacks stay bounded at 128. Other crop
families are registered placeable content; farming actions operate on barley.

Wild crops and groves are separate contributors. Their `10_`/`20_` keys set
lexical precedence; both sample builtin terrain. Negative chunks use the same
absolute-coordinate grid rules. Seasons keep their own bytes and deadlines.
Phase edges order planners without granting another system's state or output.

Client startup supplies the downloaded panel heading. Preview it with:

```sh
cargo run --release -- ui-preview /tmp/farming-panel fixtures/farming-scale/packages
```

`cargo test farming_scale_downloads_plants_harvests_and_recovers_three_systems`
covers the real listener, download, finite actions, owner wakes and restart.
Use fresh saves after persistent declaration changes; no migration is supplied.
`python3 fixtures/farming-scale/generate.py` rebuilds the checked-in package
without external dependencies. The font and OFL license match the combined mod.

## Capacity pressure inputs

Generate synthetic valid content into a new temporary directory:

```sh
python3 fixtures/farming-scale/generate-pressure.py /tmp/farming-pressure-packages
```

Defaults create four packages with 1,024 modules and 1,024 valid 64×64 RGBA
textures, approximately 16 MiB of discovered files. This crosses the old
aggregate module, asset and 4 MiB ceilings. These exercise admission/delivery;
they are not ordinary farming gameplay. Options isolate boundary cases:
`--modules 257`, `--assets 257`, `--packages 5`, or larger `--image-size`.
Decoded texture/preparation budgets remain independent; discovery alone does
not prove an installation can prepare or render within every downstream budget.

Run the explicit maximum-count real-listener acceptance probe after generation:

```sh
BLOXGLOOM_PRESSURE_PACKAGES=/tmp/farming-pressure-packages cargo test farming_pressure_maximum_counts -- --ignored --nocapture --test-threads=1
```

It verifies client-visible counts, download, source preparation and catalog
agreement before `Welcome`. Textures are valid synthetic unregistered assets;
it reports their source pixel bytes without claiming GPU allocation. For the
mixed finite-action/download workload, generate three pressure packages, copy
`packages/farm` alongside them, then set `BLOXGLOOM_FARMING_PRESSURE_PACKAGES` to
that combined root while running `cargo test farming_scale_mixed_load --
--nocapture --test-threads=1`. This crosses the old installation ceilings while
leaving room for the gameplay fixture within the new 1,024-module cap.
