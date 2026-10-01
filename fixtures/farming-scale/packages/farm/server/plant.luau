--!strict
local recipe = import("farm:recipe_barley")
return function(c: BloxGameplayContext, e: BloxActionEvent): ()
    assert(e.cell ~= nil)
    local selected = c.inventory("player")[e.slot + 1].stack
    assert(selected ~= nil and selected.item == recipe.seed, "Select barley seeds")
    assert(c.take("player", e.slot, recipe.planting_cost) ~= nil, "Need one barley seed")
    c.set_block(e.cell[1], e.cell[2], e.cell[3], "farm:barley_seedling")
end
