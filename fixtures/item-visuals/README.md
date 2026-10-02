# Stack-dependent item visuals

Start an isolated world with:

```sh
cargo run --release -- server-packages fixtures/item-visuals/packages 127.0.0.1:4000 /tmp/bloxgloom-item-visual-world
cargo run --release -- client 127.0.0.1:4000
```

Use the admin item picker to give yourself the `Charge cell` item. Counts below
64 show orange fills; counts 64–128 show green fills. World-drop art scales from
roughly half size to 1.5 times the ordinary size as count increases. The component
schema also demonstrates blue fill and a world-drop scale of 1.25 when its first
byte is 255. Both world drops and pickup flights use the exact received components;
tests exercise that variant without adding an unrelated gameplay command for
manufacturing it.

See `docs/modding/item-visuals.md` for the callback contract and bounds.
