--!strict
local recipe = import("farm:recipe_barley")
return function(c: BloxGameplayContext, e: BloxActionEvent): ()
    assert(e.cell ~= nil)
    -- Both insertion and edit roll back if either fails; host stack cap is 128.
    assert(c.give("player", {item=recipe.produce,count=recipe.harvest_yield}), "Need inventory room")
    c.set_block(e.cell[1], e.cell[2], e.cell[3], "bloxgloom:stone")
end
