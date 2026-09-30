--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxActionEvent)
    assert(e.command_arguments)
    local target = c.player_by_session(e.command_arguments[1] :: BloxSessionId)
    assert(target)
    local mode = e.command_arguments[2] :: number
    assert(mode == 1 or mode == 2 or mode == 3, "Mode must be 1 (uniform), 2 (builder) or 3 (ban)")
    if mode == 1 then
        c.set_player_appearance(target.session, 1, 2, 3)
        c.message_player(target.session, "Server uniform applied")
    elseif mode == 2 then
        c.set_profile_state("welcome:policy", target.profile, "role:builder", "builder")
        c.message_player(target.session, "Welcome policy grants you its builder role")
    else
        c.set_profile_state("welcome:policy", target.profile, "banned", "")
        c.kick_player(target.session, "This profile is banned by welcome policy")
    end
end
