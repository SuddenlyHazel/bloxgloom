--!strict
-- @module @bloxgloom
return function(h: BloxClientPlayerHost, e: BloxClientPlayerEvent)
    if e.kind == "SessionDisconnected" then
        log.info("player session retired", {session = tostring(e.session), reason = e.reason})
        return
    end
    local progress = e.states["welcome:progress"]
    local label = if progress and progress.public ~= "" then progress.public else "initializing"
    h.set_text("welcome:progress/status", "Profile progress: " .. label)
end
