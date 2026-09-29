return function(input)
    if input.event == 'uitarget:pat' then
        return {{op = 'action', key = 'uitarget:pat', arguments = string.char(7, 9)}}
    end
    return {}
end
