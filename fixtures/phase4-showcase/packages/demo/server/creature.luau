return function(context)
    if context.event == 'interact' then
        assert(context.request == 'pat')
        return 'happy'
    end
    return context.data, 10, nil, nil
end
