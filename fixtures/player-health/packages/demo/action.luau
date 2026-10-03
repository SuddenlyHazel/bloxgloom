return function(c, e)
    local player = c.player_by_profile(c.player_profile)
    local health = c.player_health(player.session)
    local amount = e.command_arguments[1] or 25
    if e.action == "demo:hurt" then
        c.damage_player(player.session, health.revision, amount, "demo:self")
    elseif e.action == "demo:heal" then
        c.heal_player(player.session, health.revision, amount)
    elseif e.action == "demo:kill" then
        c.damage_player(player.session, health.revision, 1000000, "demo:self")
    end
    local result = c.player_health(player.session)
    if result.alive then
        c.message_player(player.session, "Health " .. result.current .. " / " .. result.max)
    end
end
