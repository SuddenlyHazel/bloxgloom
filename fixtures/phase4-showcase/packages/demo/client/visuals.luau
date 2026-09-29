return function(input)
    if input.event ~= 'replica:entities' then return {} end
    local commands = {}
    for _, entity in input.entities do
        if #commands >= 16 then break end
        assert(entity.key == 'demo:sproutling' and #entity.public == 5)
        local grounded = entity.public:byte(5) == 1
        commands[#commands + 1] = {
            op = 'tint', id_lo = entity.id_lo, id_hi = entity.id_hi,
            r = if grounded then 0.7 else 1.0,
            g = 1.0,
            b = if grounded then 0.7 else 0.5,
        }
        commands[#commands + 1] = {
            op = 'spark', id_lo = entity.id_lo, id_hi = entity.id_hi,
            x = 0, y = 0.55, z = 0,
            r = 0.25, g = 0.85, b = 1.0,
        }
    end
    return commands
end
