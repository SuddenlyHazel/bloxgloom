--!strict
-- @module @bloxgloom
return function(c: BloxGameplayContext, e: BloxActionEvent)
    assert(e.command_arguments)
    local target = c.player_by_session(e.command_arguments[1] :: BloxSessionId)
    assert(target)
    local mode = e.command_arguments[2] :: number
    assert(mode >= 1 and mode <= 4, "Mode must be 1 (uniform), 2 (builder), 3 (ban) or 4 (send item)")
    if mode == 1 then
        c.set_player_appearance(target.session, 1, 2, 3)
        c.message_player(target.session, "Server uniform applied")
    elseif mode == 2 then
        c.set_profile_state("welcome:policy", target.profile, "role:builder", "builder")
        c.message_player(target.session, "Welcome policy grants you its builder role")
    elseif mode == 3 then
        c.set_profile_state("welcome:policy", target.profile, "banned", "")
        c.kick_player(target.session, "This profile is banned by welcome policy")
    else
        assert(target.profile ~= c.player_profile, "Select another player")
        local slots = c.inventory(target.profile)
        for i, slot in slots do
            if slot.insert and c.transfer_inventory("player", e.slot, target.profile, i - 1, 1) then
                c.message_player(target.session, "An operator sent you one item")
                return
            end
        end
        error("Selected slot is empty or the recipient has no room")
    end
end
