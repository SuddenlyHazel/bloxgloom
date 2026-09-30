--!strict
-- @module @bloxgloom
type Recipe = {key: string, name: string, category: string, input: string, output: string}
local recipes: {Recipe} = {
    {key = "stone", name = "Crush stone", category = "crushing", input = "Stone", output = "Gravel"},
    {key = "gravel", name = "Press gravel", category = "pressing", input = "Gravel", output = "Stone"},
}
return function(input: BloxUiInput): {BloxUiCommand}
    local selected = if input.state == "" then "stone" else input.state
    if input.event == "recipe:select" then
        selected = string.match(input.node or "", "choose_(%a+)$") or selected
    end
    local search = string.lower(tostring(input.values["recipe:browser/search"] or ""))
    local category = tostring(input.values["recipe:browser/category"] or "all")
    local selectedOnly = input.values["recipe:browser/selected_only"] == true
    local quantityValue = input.values["recipe:browser/quantity"]
    local quantity = if type(quantityValue) == "number" then quantityValue else 1
    local commands: {BloxUiCommand} = {{op = "state", value = selected}}
    if input.event == "recipe:craft" then
        table.insert(commands, {op = "action", key = "recipe:craft", arguments = selected .. ":" .. tostring(quantity)})
        return commands
    end
    local nodes: {BloxUiNode} = {}
    for _, recipe in ipairs(recipes) do
        if (category == "all" or category == recipe.category)
            and (not selectedOnly or recipe.key == selected)
            and string.find(string.lower(recipe.name), search, 1, true) ~= nil then
            local parent = #nodes
            table.insert(nodes, {id = "row_" .. recipe.key, kind = "panel", style = "recipe:row"})
            table.insert(nodes, {id = "choose_" .. recipe.key, parent = parent, kind = "button", style = "recipe:button", text = (if recipe.key == selected then "* " else "") .. recipe.name, event = "recipe:select"})
            table.insert(nodes, {id = "cost_" .. recipe.key, parent = parent, kind = "label", style = "recipe:label", text = tostring(quantity) .. " " .. recipe.input .. " -> " .. tostring(quantity) .. " " .. recipe.output})
        end
    end
    table.insert(commands, {op = "children", node = "recipe:browser/recipes", nodes = nodes})
    table.insert(commands, {op = "text", node = "recipe:browser/selection", value = "Selected: " .. selected .. ", quantity " .. tostring(quantity)})
    return commands
end
