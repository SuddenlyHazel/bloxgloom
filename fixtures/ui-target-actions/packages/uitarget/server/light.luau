--!strict
-- Cost and world edit are staged together by the authoritative server.
return function(context: BloxGameplayContext, event: BloxActionEvent): ()
    assert(event.kind == 'ActionRequested' and event.action == 'uitarget:light')
    assert(event.cell ~= nil and event.entity == nil and event.arguments == '')
    local stack = context.inventory('player')[event.slot + 1].stack
    assert(stack ~= nil and stack.item == 'bloxgloom:stick', 'Select a stick to light stone')
    assert(context.take('player', event.slot, 1) ~= nil, 'Need one stick')
    context.set_block(event.cell[1], event.cell[2], event.cell[3], 'bloxgloom:glowstone')
end
