return function(context)
    if context.event == 'interact' then
        assert(context.request == 'pat')
        local pats = tonumber(context.data:match('^happy:(%d+)$')) or 0
        return 'happy:' .. tostring(pats % 9 + 1)
    end
    return context.data, 10, nil, nil
end
