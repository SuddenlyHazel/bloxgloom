--!strict
return function(c: BloxOwnerContext): (string, number)
    -- A bounded demonstration bed in the seeded owner's authoritative chunk.
    for x=2, 5 do
        local before = c.block(x, 81, 0)
        for index, stage in ipairs({"seedling", "young", "flowering"}) do
            if before == "farm:barley_" .. stage then
                local after = ({"young", "flowering", "ripe"})[index]
                c.edit(x, 81, 0, before, "farm:barley_" .. after)
            end
        end
    end
    return tostring((tonumber(c.data) or 0) + 1), 1000
end
