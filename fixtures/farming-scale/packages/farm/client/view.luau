--!strict
return function(input: BloxUiInput): {BloxUiCommand}
    if input.event == "farm:plant" or input.event == "farm:harvest" then
        return {{op="action",key=tostring(input.event)}}
    end
    return {}
end
