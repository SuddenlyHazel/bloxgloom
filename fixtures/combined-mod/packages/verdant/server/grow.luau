-- This separate durable chunk owner wakes after a logical deadline, not a
-- window timer. The seeded owner only changes a designated stone cell once.
return function(c)
    if c.data == '0' then
        local before = c.block(3,80,0)
        if before == 'bloxgloom:stone' then
            c.edit(3,80,0,before,'verdant:jade')
        end
        return '1', 120
    end
    return c.data, 120
end
