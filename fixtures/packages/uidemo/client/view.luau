-- Fresh sandbox per event. Explicit string state is local to this document.
return function(input)
    if input.event == "uidemo:name-changed" then
        return {{op = "text", node = "uidemo:welcome/title", value = "Garden: " .. string.sub(input.value, 1, 120)}}
    end
    if input.event == "uidemo:plant" then
        local title = "Seed planted!"
        if input.state == "planted" then title = "Another seed planted!" end
        return {
            {op = "text", node = "uidemo:welcome/title", value = title},
            {op = "text", node = "uidemo:welcome/confirm", value = "Plant another seed"},
            {op = "visible", node = "uidemo:welcome/hint", value = false},
            {op = "state", value = "planted"},
        }
    end
    return {}
end
