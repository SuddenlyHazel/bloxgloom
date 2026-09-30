--!strict
-- @module @bloxgloom
type Recipe = {key: string, name: string, category: string, input: string, output: string}
local recipes: {Recipe} = {
    {key = "stone", name = "Crush stone", category = "crushing", input = "Stone", output = "Gravel"},
    {key = "gravel", name = "Press gravel", category = "pressing", input = "Gravel", output = "Stone"},
}
local function plain(stack: BloxStack): boolean
    return stack.components == nil or (stack.components.version == 0 and stack.components.bytes == "")
end

return function(input: BloxUiInput | BloxReplicaInput): {BloxUiCommand}
    local replicaEvent = string.sub(input.event, 1, 8) == "replica:"
    if replicaEvent and input.texts["recipe:browser/selection"] == nil then
        return {} -- This package's observer can run while another document is open.
    end
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
    if input.event == "recipe:craft" and not replicaEvent then
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
    local inventory = input.replica.inventory
    local availability = "Inventory: waiting for server"
    local components = "No tagged input"
    if inventory ~= nil then
        local inputKey = if selected == "stone" then "bloxgloom:stone" else "bloxgloom:gravel"
        local outputKey = if selected == "stone" then "bloxgloom:gravel" else "bloxgloom:stone"
        local available, capacity = 0, 0
        for _, slot in ipairs(inventory.slots) do
            local stack = slot.stack
            if stack == nil then
                capacity += 128
            elseif plain(stack) then
                if stack.item == inputKey then
                    available += stack.count
                elseif stack.item == outputKey then
                    capacity += 128 - stack.count
                end
            elseif stack.item == inputKey and stack.components ~= nil then
                local payload = stack.components
                components = "Tagged input excluded: " .. tostring(stack.count) .. "; v" .. tostring(payload.version)
                    .. " bytes " .. tostring(string.byte(payload.bytes, 1) or "-") .. "/" .. tostring(string.byte(payload.bytes, 2) or "-")
            end
        end
        local maximum = 0
        for _, slot in ipairs(inventory.slots) do
            local stack = slot.stack
            if stack ~= nil and plain(stack) and stack.item == inputKey then
                for count = math.min(8, stack.count), maximum + 1, -1 do
                    local room = capacity + (if count == stack.count then 128 else 0)
                    if count <= room then
                        maximum = count
                        break
                    end
                end
            end
        end
        availability = "Plain input: " .. tostring(available) .. "; max from a matching slot: " .. tostring(maximum)
    end
    table.insert(commands, {op = "text", node = "recipe:browser/availability", value = availability})
    table.insert(commands, {op = "text", node = "recipe:browser/components", value = components})
    local world = input.replica.world
    table.insert(commands, {op = "text", node = "recipe:browser/world", value = if world == nil then "World: waiting for server" else "World cycle: " .. tostring(world.elapsed_ms) .. "/" .. tostring(world.cycle_ms) .. " ms"})
    local actions = input.replica.actions
    local latest = actions[#actions]
    table.insert(commands, {op = "text", node = "recipe:browser/receipt", value = if latest == nil then "No craft receipt yet" else "Craft receipt: " .. (if latest.accepted then "accepted" else "denied")})
    table.insert(commands, {op = "text", node = "recipe:browser/receipt_id", value = if latest == nil then "" else latest.id})
    return commands
end
