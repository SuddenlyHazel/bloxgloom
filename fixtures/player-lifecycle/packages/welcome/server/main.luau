--!strict
-- @module @bloxgloom
return function(h: BloxStartupHost)
    h.register_player_lifecycle("welcome:progress", 1, 64, "", "welcome:progress")
    h.register_committed_observer("welcome:audit", 1, "welcome:audit")
    local playerArgument: BloxCommandArgument = {kind = "player"}
    h.register_action("welcome:inspect", 1, "Inspect player", "empty", nil, "welcome:inspect", {
        permission = "Player", arguments = {playerArgument},
    })
    h.register_action("welcome:uniform", 1, "Apply server uniform", "empty", nil, "welcome:uniform", {
        permission = "Admin", arguments = {playerArgument},
    })
end
