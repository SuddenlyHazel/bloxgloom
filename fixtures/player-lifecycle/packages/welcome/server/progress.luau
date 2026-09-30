--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxPlayerLifecycleEvent): BloxPlayerLifecycleDecision?
    if e.kind == "PlayerJoined" then
        if e.player then c.message_player(e.player.session, "Welcome to the world") end
        if e.state == "" then
            assert(c.give("player", {item = "bloxgloom:stick", count = 3}))
            return {state = "kit", public_state = "level:1", session_state = "connected"}
        end
        return {session_state = "connected"}
    elseif e.kind == "PlayerSpawned" then
        log.info("player admitted", {session = if e.player then tostring(e.player.session) else "offline"})
    end
    return nil
end
