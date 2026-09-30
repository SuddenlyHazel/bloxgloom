--!strict
-- The server checks sight and block type before this callback. The stick and
-- block edit participate in the same authoritative gameplay transaction.
return function(context: BloxGameplayContext, event: BloxActionEvent): ()
    assert(event.kind == 'ActionRequested' and event.action == 'verdant:plant')
    assert(event.cell ~= nil and event.entity == nil and event.arguments == '')
    local stack = context.inventory('player')[event.slot + 1].stack
    assert(stack ~= nil and stack.item == 'bloxgloom:stick', 'Select a stick')
    assert(context.take('player', event.slot, 1) ~= nil, 'Need one stick')
    context.set_block(event.cell[1], event.cell[2], event.cell[3], 'verdant:jade')
    -- Attempt diagnostics survive failure; this does not claim the edit committed.
    log.debug('Plant proposed', {cost=1, sample=math.random(), x=event.cell[1], y=event.cell[2], z=event.cell[3]})
end
