--!strict
-- @module @bloxgloom
-- Ephemeral diagnostics, not a durable receipt or gameplay counter.
local observed = 0
return function(e: BloxCommittedEvent)
    if e.inventory then
        observed += 1
        log.info("inventory committed", {
            profile = tostring(e.inventory.profile),
            revision = tostring(e.inventory.revision),
            observed = observed,
        })
    end
end
