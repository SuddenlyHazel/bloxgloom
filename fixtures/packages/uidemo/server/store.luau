-- An exact one-item move, entirely staged and authorized by the server.
return function(context, event)
    assert(event.kind == 'ActionRequested' and event.action == 'uidemo:store')
    assert(event.arguments == '')
    assert(event.slot ~= 1 and context.transfer(event.slot, 1, 1), 'Need a stick outside slot 2 and room in slot 2')
end
