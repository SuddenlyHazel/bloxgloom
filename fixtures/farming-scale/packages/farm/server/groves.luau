--!strict
return function(c: BloxGenerationContext): ()
    -- Absolute grid decisions also agree for negative chunks and seams.
    for x=0,15 do for z=0,15 do
        local wx, base, wz = c.world_position(x,0,z)
        local y = c.builtin_terrain_height(wx,wz) + 1
        local ly = y - base
        if ly >= 0 and ly < 16 and wx % 8 == 0 and wz % 8 == 0 then
            c.set_block(x,ly,z,"farm:blueberry_ripe")
        end
    end end
end
