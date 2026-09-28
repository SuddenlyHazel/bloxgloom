return function(input)
    if input.event == 'uitarget:light' then
        return {{op = 'action', key = 'uitarget:light'}}
    end
    return {}
end
