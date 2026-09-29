--!strict
return function(input: BloxUiInput): {BloxUiCommand}
    if input.event == 'verdant:plant' then
        return {{op='action', key='verdant:plant'}}
    end
    return {}
end
