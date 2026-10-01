--!strict
return function(input: BloxReplicaInput): {BloxReplicaCommand}
    if input.event ~= "replica:entities" then return {} end
    local commands: {BloxReplicaCommand} = {}
    for _, entity in input.entities do
        if #commands >= 16 then break end
        if entity.key == "throw:seed_body" or entity.key == "throw:guided_body" then
            commands[#commands + 1] = {op = "spark", entity = entity.id,
                x = 0, y = 0, z = 0, r = 0.3, g = 0.8, b = 1.0, size = 0.06, lifetime_ms = 120}
        end
    end
    return commands
end
