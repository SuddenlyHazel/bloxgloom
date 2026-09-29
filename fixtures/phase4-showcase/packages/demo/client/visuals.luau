return function(input)
    if input.event ~= 'replica:entities' then return {} end
    local commands = {}
    for _, entity in input.entities do
        assert(entity.key == 'demo:sproutling' and #entity.public == 5)
        local grounded = entity.public:byte(5) == 1
        commands[#commands + 1] = {
            op = 'tint', id_lo = entity.id_lo, id_hi = entity.id_hi,
            r = if grounded then 0.7 else 1.0,
            g = 1.0,
            b = if grounded then 0.7 else 0.5,
        }
    end
    return commands
end
