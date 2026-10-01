--!strict
-- Presentation module locals survive events in this connection's UI realm.
-- Explicit string state remains local to this document.
return function(input: BloxUiInput): {BloxUiCommand}
    if input.event == "uidemo:name-changed" then
        return {{op = "text", node = "uidemo:welcome/title", value = "Garden: " .. string.sub(input.value, 1, 120)}}
    end
    if input.event == "uidemo:store" then
        return {{op = "action", key = "uidemo:store"}}
    end
    return {}
end
