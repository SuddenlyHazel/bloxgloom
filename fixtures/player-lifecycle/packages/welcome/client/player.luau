--!strict
-- @module @bloxgloom
-- This module belongs to the connection's player-service realm. Its controller
-- and cache survive snapshots; reconnect initializes a new copy.
local labels: {[string]: string} = {}
local updates = coroutine.create(function()
    local count = 0
    while true do
        count += 1
        coroutine.yield(count)
    end
end)

return function(h: BloxClientPlayerHost, e: BloxClientPlayerEvent)
    if e.kind == "SessionDisconnected" then
        log.info("player session retired", {session = tostring(e.session), reason = e.reason})
        return
    end
    -- Resume manually from a host callback. The coroutine retains no host object
    -- and has no automatic timer or background authority.
    local ok, count = coroutine.resume(updates)
    assert(ok, tostring(count))
    local progress = e.states["welcome:progress"]
    local label = if progress and progress.public ~= "" then progress.public else "initializing"
    local text = labels[label]
    if not text then
        text = "Profile progress: " .. label
        labels[label] = text
    end
    h.set_text("welcome:progress/status", text)
    log.debug("local progress controller updated", {callbacks = count})
end
