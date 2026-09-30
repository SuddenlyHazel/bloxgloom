--!strict
-- @module @bloxgloom
return function(h: BloxStartupHost)
    h.register_action("recipe:craft", 1, "Craft selected recipe", "empty", nil, "recipe:craft")
end
