--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxActionEvent)
    local arguments=e.command_arguments
    assert(arguments)
    local target=c.player_by_session(arguments[1] :: BloxSessionId)
    assert(target)
    c.set_player_appearance(target.session,1,2,3)
    c.message_player(target.session,"Your server uniform is ready")
end
