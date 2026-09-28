-- The source observes its east neighbor, then durably invites that owner.
-- That owner runs on a later tick and conditionally edits its own east neighbor.
return function(c)
    if c.data == 'seed' then
        assert(#c.inbox == 0)
        local observed = c.block(18, 80, 2)
        c.send(1, 5, 0, observed)
        return 'sent', 4294967295
    end
    if c.data == 'new' then
        assert(#c.inbox == 1)
        local message = c.inbox[1]
        assert(message.produced_tick_hi < c.tick_hi or
            (message.produced_tick_hi == c.tick_hi and message.produced_tick_lo < c.tick_lo))
        local before = c.block(34, 80, 2)
        if before == message.payload and before == 'bloxgloom:air' then
            c.edit(34, 80, 2, before, 'bloxgloom:glowstone')
            -- Reads remain the captured preimage, not proposed output.
            assert(c.block(34, 80, 2) == before)
        end
        return 'done', 4294967295
    end
    return c.data, 4294967295
end
