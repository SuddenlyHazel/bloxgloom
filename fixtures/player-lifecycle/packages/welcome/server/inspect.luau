--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxActionEvent)
    local arguments = e.command_arguments
    assert(arguments)
    local target = c.player_by_session(arguments[1] :: BloxSessionId)
    assert(target)
    local policy = c.profile_state("welcome:policy", target.profile)
    log.info("inspected player", {name = target.name, session = tostring(target.session), policy = policy.state})
end
