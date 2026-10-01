--!strict
return function(c: BloxOwnerContext): (string, number)
    c.wake("farm:growth", 0, 5, 0)
    return tostring((tonumber(c.data) or 0) + 1), 100
end
