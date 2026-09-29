--!strict
return function(input: BloxReplicaInput): {BloxReplicaCommand}
    if input.event == 'replica:anchors' then
        local commands: {BloxReplicaCommand} = {}
        for _, entity in input.entities do
            if entity.key == 'demo:press_machine' then
                assert(entity.motion_revision:is_initial())
                commands[#commands + 1] = {
                    op = 'spark', entity = entity.id,
                    x = 0, y = 0.6, z = 0,
                    r = 0.25, g = 0.85, b = 1.0,
                    size = 0.28, lifetime_ms = 1200,
                }
            end
        end
        return commands
    end
    if input.event ~= 'replica:entities' then return {} end
    local commands: {BloxReplicaCommand} = {}
    for _, entity in input.entities do
        if #commands >= 16 then break end
        assert(entity.key == 'demo:sproutling' and #entity.public == 5)
        local grounded = entity.public:byte(5) == 1
        commands[#commands + 1] = {
            op = 'tint', entity = entity.id,
            r = if grounded then 0.7 else 1.0,
            g = 1.0,
            b = if grounded then 0.7 else 0.5,
        }
        commands[#commands + 1] = {
            op = 'spark', entity = entity.id,
            x = 0, y = 0.55, z = 0,
            r = 0.25, g = 0.85, b = 1.0,
        }
    end
    return commands
end
