--!strict
-- @module @bloxgloom
return function(h: BloxClientStartupHost)
    h.set_text("welcome:progress/status", "Waiting for profile progress")
    h.set_player_handler("welcome:player")
end
