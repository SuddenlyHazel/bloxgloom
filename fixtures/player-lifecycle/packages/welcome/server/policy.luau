--!strict
-- @module @bloxgloom
return function(_c: BloxGameplayContext, e: BloxPlayerLifecycleEvent): BloxPlayerLifecycleDecision?
    if e.kind == "PlayerJoining" and e.state == "banned" then
        return {deny = "This profile is banned by welcome policy"}
    end
    return nil
end
