# Regions and chat

Packages requiring `bloxgloom:players/v1` can declare named regions and pure chat moderation hooks during startup. The runnable fixture is `fixtures/player-world/packages`.

```luau
host.register_player_lifecycle("demo:players", 1, 32, "", "demo:players")
host.register_region("demo:spawn", {-16, -256, -16}, {16, 512, 16}, "demo:players")
host.register_chat_hook("demo:moderation", 1, "demo:chat")
```

Region bounds include each minimum and exclude each maximum. Membership uses the authoritative player's feet position; overlap produces independent transitions for each region. Admission, accepted movement, teleport and departure reconcile membership. RegionEntered and RegionLeft callbacks receive `event.region` and the ordinary exact player/session handles. They run through the player lifecycle WAL path: profile writes and rewards commit together, and session operations cannot target a replacement session. RegionEntered callbacks are discarded when their captured session has departed. Under callback queue pressure membership reconciliation incrementally retries; endpoint changes occurring entirely during sustained overload may coalesce. A teleport crossing a region without ending inside it does not enter that region.

A package may declare 32 regions, and an installation may contain 256. Each region references an existing lifecycle service in its own package. Bounds must be finite, increasing, and within ±1,000,000 blocks. Region definitions and chat hook contracts are transient startup rules, compared during live reload; their names are not persistent content numeric IDs.

A chat module returns a function receiving immutable `{sender, text, players, identity_trust}`. Sender names/profiles/sessions come from the server-admitted connection; clients cannot submit sender identities. The current profile trust remains `claimed_profile`, because this API does not add account authentication. No world/inventory mutation host is available to moderators.

```luau
return function(event)
    if event.text == "spoiler" then
        return {allow = false, reason = "Please avoid spoilers."}
    end
    return {allow = true, text = event.text, recipients = {event.sender}}
end
```

Allow requires explicit `allow = true` and valid replacement text. Deny requires `allow = false` and a reason. Omitting `recipients` broadcasts to admitted sessions. Recipient entries are player views containing exact profile and session handles; stale, mismatched or fabricated recipients reject the message. Multiple hooks run in lexical key order; routing can only narrow the captured audience. Any hook error, invalid decision or denial prevents delivery. Moderation runs on a bounded background worker, and publication revalidates sender and recipient sessions. The queue holds 16 pending requests and 16 completed results; saturation produces a busy response. Chat is transient, best effort, and never blocks world publication.

Text and denial reasons allow 1–512 UTF-8 bytes, exclude control characters, and must contain a non-whitespace character. Each session may submit four requests per two seconds. Monotonic request sequences suppress replays, including denied and throttled requests. The installation supports 32 chat hooks, up to eight per package.

In game, T or Enter opens chat, Enter sends, Escape closes, and Up/Down browse the last 32 sent messages. The transcript retains 64 lines. Opening chat clears held movement and releases the cursor. Messages are not written to external services or save files.
