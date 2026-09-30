--!strict
-- @module @bloxgloom
return function(h: BloxStartupHost)
    h.register_player_lifecycle("welcome:progress", 1, 64, "", "welcome:progress")
    local playerArgument: BloxCommandArgument = {kind = "player"}
    h.register_action("welcome:inspect", 1, "Inspect player", "empty", nil, "welcome:inspect", {
        permission = "Player", arguments = {playerArgument},
    })
end
