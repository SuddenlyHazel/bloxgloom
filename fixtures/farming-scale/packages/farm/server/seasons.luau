--!strict
return function(c: BloxOwnerContext): (string, number)
    local phase = (tonumber(c.data) or 0) % 22
    local calendar = import("farm:calendar_" .. string.format("%02d", phase))
    return tostring(calendar.next_phase), calendar.duration
end
