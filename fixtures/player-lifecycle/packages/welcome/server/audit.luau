--!strict
-- @module @bloxgloom
return function(e: BloxCommittedEvent)
    if e.inventory then
        log.info("inventory committed", {
            profile = tostring(e.inventory.profile),
            revision = tostring(e.inventory.revision),
        })
    end
end
