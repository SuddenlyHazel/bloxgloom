-- This callback shares the originating transaction. It cannot recursively change
-- health, but can stage normal package-owned inventory/state effects if desired.
return function(c, event)
    if event.kind == "died" then
        c.message_player(event.player.session, "You died. Inventory kept; click Respawn to return.")
    elseif event.kind == "respawned" then
        c.message_player(event.player.session, "Respawned with " .. event.health.current .. " health.")
    end
end
