return function(input)
    if input.event == 'verdant:plant' then
        return {{op='action', key='verdant:plant'}}
    end
    return {}
end
