return function(c,e)
    local own = e.action == "pace:self"
    local player = if own then c.player_by_profile(c.player_profile) else c.player_by_session(e.command_arguments[1])
    assert(player, "Player is no longer connected")
    local mode = e.command_arguments[if own then 1 else 2]
    if mode==1 then
        c.set_player_modifier(player.profile,"pace:training",{speed=1.25})
    elseif mode==2 then
        c.set_player_modifier(player.session,"pace:slow",{speed=0.5,duration_ticks=600})
    elseif mode==3 then
        c.remove_player_modifier(player.profile,"pace:training")
        c.remove_player_modifier(player.session,"pace:slow")
    else
        error("Mode must be 1 (saved boost), 2 (temporary slow) or 3 (clear)")
    end
    local message = if mode == 1 then "Saved walking speed boost: +25%"
        elseif mode == 2 then "Temporary speed multiplier: 50% for 600 ticks"
        else "Movement effects cleared"
    c.message_player(player.session, message)
end
