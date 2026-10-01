--!strict
return function(c: BloxGameplayContext, e: BloxGameplayEvent): ()
    if e.kind == "MovingTick" then return end
    if e.kind == "MovingImpact" then
        local target = e.target
        if target.kind ~= "Terrain" or e.normal[2] ~= 1 or e.blocked then return end
        local cell = target.cell
        local x, y, z = cell[1], cell[2] + 1, cell[3]
        -- React to captured current terrain, rather than trusting old target text.
        local support = c.block(cell[1], cell[2], cell[3]).state
        if (support == "bloxgloom:stone" or support == "bloxgloom:grass")
            and c.block(x, y, z).state == "bloxgloom:air" then
            c.set_block(x, y, z, "bloxgloom:tall_grass")
            assert(c.remove_entity(e.entity))
        end
    elseif e.kind == "MovingExpiry" then
        local pose = c.motion(e.entity)
        assert(pose ~= nil)
        -- Return the one consumed seed if no plant was placed.
        c.spawn_drop(pose.position[1], pose.position[2], pose.position[3], "throw:seed", 1, 0)
        assert(c.remove_entity(e.entity))
    end
end
