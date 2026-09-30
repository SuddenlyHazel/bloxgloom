--!strict
-- @module @bloxgloom
-- UI selection is a request. These server-owned recipes enforce the economy.
return function(c: BloxGameplayContext, e: BloxActionEvent)
    assert(e.kind == "ActionRequested" and e.action == "recipe:craft")
    local recipe, digits = string.match(e.arguments, "^(%a+):(%d+)$")
    local count = tonumber(digits)
    assert(count ~= nil and count >= 1 and count <= 8 and count % 1 == 0, "Invalid recipe quantity")
    local input: string
    local output: string
    if recipe == "stone" then
        input, output = "bloxgloom:stone", "bloxgloom:gravel"
    elseif recipe == "gravel" then
        input, output = "bloxgloom:gravel", "bloxgloom:stone"
    else
        error("Unknown recipe")
    end
    local stack = c.inventory("player")[e.slot + 1].stack
    assert(stack ~= nil and stack.item == input and stack.count >= count, "Select enough recipe input")
    assert(stack.components == nil or (stack.components.version == 0 and stack.components.bytes == ""), "Recipe needs component-free input")
    assert(c.take("player", e.slot, count) ~= nil, "Cannot consume recipe input")
    assert(c.give("player", {item = output, count = count}), "No room for recipe output")
end
