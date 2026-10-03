return function(c)
    local player = c.player_by_profile(c.player_profile)
    assert(player and player.model)
    c.play_player_animation(player.session, {clip = "nod", looping = false, crossfade_ms = 100})
end
