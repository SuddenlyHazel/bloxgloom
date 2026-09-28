# Player rules

A package with `bloxgloom:content/v1` may select one startup-frozen player
contract for the whole world:

```luau
return function(host)
    host.register_player_rules('demo:lightweight', 1, {
        half_width = 0.2, foot_inset = 0.025,
        middle_height = 0.4, head_height = 0.75,
        intent_blocks_per_second = 2, budget_blocks_per_second = 3,
        headroom = 4, max_rise = 8, eye_height = 0.65,
    })
end
```

The key must belong to the declaring package. Exactly one package may select
rules. The host rejects omitted, nonfinite or out-of-range fields; the server
still owns collision, input ordering, world reads and spawn admission. Rules
affect both authoritative movement and client prediction, camera, reach and
placement checks. They are not a per-client speed setting.

The selection's key, revision and every numeric field participate in the
persisted player identity and the negotiated client catalog. A client installs
the verified package bundle and checks that identity before entering play.
Changing or removing the selection on an existing save fails closed; while
developing an incompatible contract, start a fresh world directory. Without a
selection the builtin rules and their previous catalog identity remain in use.

Player cosmetic appearance is separate from collision rules. A content package
may append up to 24 colors per category to the existing skin/shirt/pants palettes:

```luau
host.register_player_appearance('demo:wardrobe', 1,
    'bloxgloom:humanoid/v1', {
        skins = {{0.1, 0.9, 0.2}},
        shirts = {{0.9, 0.1, 0.2}},
        pants = {{0.1, 0.2, 0.9}},
    })
```

Colors are finite linear RGB in `[0, 1]`; the builtin indices and model remain
unchanged. The model string is an identity fence, not support for custom geometry.
The declaration is part of `content.map` and the verified client bundle. In the
command screen, `appearance <skin> <shirt> <pants>` selects the advertised palette
indices for your authenticated profile. The server validates them, saves them in
`players/<profile>.appearance` and replicates the four-byte public cosmetic state;
it never accepts raw client colors or another profile ID. The local optional
`BLOXGLOOM_APPEARANCE=skin,shirt,pants` join preference performs the same request.
