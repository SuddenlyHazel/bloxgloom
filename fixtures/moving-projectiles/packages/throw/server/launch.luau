--!strict
return function(c: BloxGameplayContext, e: BloxActionEvent): ()
    local p = c.player_position
    assert(p ~= nil, "Launch requires an active player")
    local stack = c.inventory("player")[e.slot + 1].stack
    assert(stack ~= nil and stack.item == "throw:seed", "Select a throwing seed")
    assert(c.take("player", e.slot, 1) ~= nil, "Need one throwing seed")
    local guided = e.action == "throw:launch_guided"
    c.spawn_moving_entity(if guided then "throw:guided_body" else "throw:seed_body", {
        -- Center origin: launch east and above the player's feet.
        position = {p[1] + 0.8, p[2] + 1.6, p[3]},
        velocity = if guided then {8, 0, 0} else {8, 6, 0},
        state = string.char(0),
    })
end
