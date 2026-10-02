# Typed commands and aliases

An empty-target gameplay action may declare a command facet in its seventh
`register_action` argument. The server validates the frozen schema and permission
before calling the handler; UI discovery is not authorization.

```lua
h.register_action("weatherlab:tune", 1, "Tune", "empty", nil, "weatherlab:tune", {
    permission = "Admin",
    aliases = { "tune" },
    arguments = {
        { kind = "text", max_bytes = 48 },
        { kind = "integer", min = -10, max = 10 },
        { kind = "number", min = 0, max = 1 },
    },
})
```

The command `tune "rain intensity" -2 0.5` sends the canonical
`weatherlab:tune` action key. The handler's readonly `event.command_arguments`
contains the string, integer, and number in schema order. Existing `player`,
`item_key`, `entity_key`, and `count` argument kinds remain supported.

- `text` requires `max_bytes` from 1–128. Values must be nonempty UTF-8 without
  control characters; limits measure encoded bytes, not characters.
- `integer` requires inclusive `min`/`max` within ±9,007,199,254,740,991, preserving
  exact values when passed into Luau. Fractional values are rejected.
- `number` requires finite inclusive `min`/`max`. NaN and infinity are rejected;
  negative zero is normalized before sending. Wire values use eight bytes.
- Up to eight arguments are allowed. The worst-case request must fit the existing
  interaction payload limit; a schema can be rejected even if a small example fits.
- Only trailing `count` arguments can have defaults. Numeric and text arguments
  always require explicit values.
- Up to four aliases are allowed, each 1–32 lowercase ASCII letters, digits, or
  underscores. Aliases must be unique across the installation. `help`, `time`,
  `weather`, `appearance`, `give`, and `spawn` remain reserved client commands.

Double quotes preserve spaces in the command entry. Backslash escapes a double
quote or a backslash. There is no shell interpolation. An unmatched quote or an
unsupported escape is rejected. Player completion also recognizes declared aliases.

Aliases are client convenience names. They are never server action identities;
forged requests using an alias fail. Changing aliases, bounds, or kinds changes
that action's compatibility identity. Existing schemas without these extensions
retain their original identity and declaration encoding. Extended declarations
use new tags that older clients reject instead of silently dropping metadata.
