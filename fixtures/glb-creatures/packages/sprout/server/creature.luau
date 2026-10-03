--!strict
return function(c: BloxCreatureContext)
    local cycleText, patsText = c.data:match('^(%d+):(%d+)$')
    local cycle = tonumber(cycleText) or 0
    local pats = tonumber(patsText) or 0
    if c.event == 'interact' then
        assert(c.request == 'pat')
        pats = (pats + 1) % 10
        local alternate = pats % 2 == 1
        c.set_variant('eyes', if alternate then 'sleepy' else 'classic')
        c.set_layer('hat', alternate)
        c.set_layer('mouth', not alternate)
        c.set_tint('body', {
            rgb = if alternate then {210, 130, 235} else {110, 210, 140},
            mode = if alternate then 'replace' else 'multiply',
        })
        c.set_tint('iris', {rgb = {80, 160, 240}, mode = 'replace'})
        c.play_animation {clip = 'nod', looping = false, crossfade_ms = 180, restart = true}
        return tostring(cycle) .. ':' .. tostring(pats)
    end
    -- A small loop around the current cell exercises automatic idle/walk blending.
    -- Movement remains a request to the server's collision solver.
    local nextData = tostring((cycle + 1) % 12) .. ':' .. tostring(pats)
    if cycle < 4 then return nextData, 10, nil, nil end
    local direction = if cycle < 8 then 1 else -1
    local p = c.position
    local target = {p[1] + direction * 0.2, p[2], p[3]}
    if c.clear(target[1], target[2], target[3]) then
        return nextData, 10, target[1], target[3]
    end
    return nextData, 10, nil, nil
end
