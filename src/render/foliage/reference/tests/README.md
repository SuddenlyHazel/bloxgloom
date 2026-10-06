# Portable BSL wind checks

`portable_oracle.wgsl` independently expresses the audited botanical wind
equations using a parameter table. It runs on the same GPU as the production
helper: recording numerical sine-hash outputs from one adapter would introduce
vendor-specific goldens. No BSL source file or artwork is embedded.

The ordinary GPU test checks the production helper and actual vertex caller
against the independent oracle at the original absolute tolerance of `1e-5`,
across all ten botanical classes, four times, two heights, two UV heights and
three eye offsets (480 cases).
Enhanced wind and pickup guards keep their exact comparisons.

For an additional live source audit, set `BLOXGLOOM_BSL_SOURCE` to a personally
supplied shader directory containing `block.properties`, `lib/` and `program/`.
The test then evaluates the separately converted GLSL against the same port and
caller cases. No source pack is required for ordinary tests or builds.
