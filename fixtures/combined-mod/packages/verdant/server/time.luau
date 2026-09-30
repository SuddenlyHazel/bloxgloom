--!strict
-- A mod command receives the same server clock operation as the stock console.
return function(context: BloxGameplayContext, event: BloxActionEvent): ()
    assert(event.action == 'verdant:noon' and event.arguments == '')
    local time = context.world_time()
    context.admin_set_time(time.cycle_ms / 4)
    assert(context.world_time().elapsed_ms == time.cycle_ms / 4)
end
