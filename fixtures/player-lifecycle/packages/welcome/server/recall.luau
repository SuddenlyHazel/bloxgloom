--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxActionEvent)
    local arguments=e.command_arguments
    local position=c.player_position
    assert(arguments and position)
    local target=c.player_by_session(arguments[1] :: BloxSessionId)
    assert(target)
    c.teleport_player(target.session,position[1],position[2],position[3])
    c.message_player(target.session,"Recalled by an administrator")
end
