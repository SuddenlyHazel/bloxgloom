--!strict
return function(c: BloxGameplayContext, _e: BloxActionEvent): ()
    local p = c.player_position
    assert(p ~= nil)
    for _, entity in c.nearby_entities(p[1], p[2], p[3], 8) do
        if entity.entity_type == "throw:seed_body" or entity.entity_type == "throw:guided_body" then
            assert(c.cancel_moving_entity(entity.id))
        end
    end
end
