--!strict
return function(c: BloxGenerationContext): ()
    for x=0,15 do for z=0,15 do
        local wx, _, wz = c.world_position(x,0,z)
        local y = c.builtin_terrain_height(wx,wz) + 1
        local _, base, _ = c.world_position(0,0,0)
        local ly = y - base
        if ly >= 0 and ly < 16 and c.random_at(wx,y,wz,41) < 0.025 then
            c.set_block(x,ly,z,"farm:barley_seedling")
        end
    end end
end
