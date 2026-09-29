--!strict
return function(context: BloxMachineContext): (string, number, boolean)
    local has_input = context.slots[2] ~= nil
    local has_fuel = context.fuel > 0 or context.slots[1] ~= nil
    return context.data, if has_input and has_fuel then 1 else 50, true
end
