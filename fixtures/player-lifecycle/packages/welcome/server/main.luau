--!strict
-- @module @bloxgloom
return function(h: BloxStartupHost)
    h.register_player_lifecycle("welcome:progress", 1, 64, "", "welcome:progress")
    h.register_player_lifecycle("welcome:policy", 1, 64, "member", "welcome:policy")
    h.register_committed_observer("welcome:audit", 1, "welcome:audit")
    local playerArgument: BloxCommandArgument = {kind = "player"}
    h.register_action("welcome:inspect", 1, "Inspect player", "empty", nil, "welcome:inspect", {
        permission = "Player", arguments = {playerArgument},
    })
    h.register_action("welcome:manage", 1, "Manage player policy", "empty", nil, "welcome:manage", {
        permission = "Admin", arguments = {playerArgument, {kind = "count", default = 1}},
    })
    h.register_action("welcome:recall", 1, "Recall player here", "empty", nil, "welcome:recall", {
        permission = "Admin", arguments = {playerArgument},
    })
end
