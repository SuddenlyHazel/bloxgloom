--!strict
return function(c: BloxGameplayContext, e: BloxGameplayEvent): ()
    if e.kind == "MovingTick" then
        local pose = c.motion(e.entity)
        assert(pose ~= nil)
        -- Durable private byte controls a repeating direction; no retained VM state.
        local state = c.entity_state(e.entity)
        assert(state ~= nil)
        local phase = (state:byte(1) + 1) % 40
        local angle = phase * math.pi / 20
        assert(c.set_motion(e.entity, pose.revision, {
            velocity = {8 * math.cos(angle), 0, 8 * math.sin(angle)},
            orientation = {0, -math.sin(angle / 2), 0, math.cos(angle / 2)},
        }))
        assert(c.update_entity(e.entity, string.char(phase)))
    elseif e.kind == "MovingExpiry" then
        local pose = c.motion(e.entity)
        assert(pose ~= nil)
        c.spawn_drop(pose.position[1], pose.position[2], pose.position[3], "throw:seed", 1, 0)
        assert(c.remove_entity(e.entity))
    end
end
