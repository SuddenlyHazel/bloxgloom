--!strict
return function(context: BloxGameplayContext, event: BloxActionEvent): ()
    assert(event.kind == 'ActionRequested' and event.action == 'uitarget:pat')
    assert(event.entity ~= nil and event.cell ~= nil)
    assert(event.arguments == string.char(7, 9), 'Invalid pat arguments')
    local stack = context.inventory('player')[event.slot + 1].stack
    assert(stack ~= nil and stack.item == 'bloxgloom:stick', 'Select a stick to pat')
    assert(context.take('player', event.slot, 1) ~= nil, 'Need one stick')
    assert(context.give('player', {item = 'bloxgloom:seeds', count = 1}), 'No room for seeds')
end
