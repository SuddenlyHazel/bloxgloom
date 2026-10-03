return function(event)
    if string.find(string.lower(event.text), "spoiler", 1, true) then
        return {allow = false, reason = "Please keep this demo spoiler free."}
    end
    if string.sub(event.text, 1, 5) == "/self" then
        return {allow = true, text = string.sub(event.text, 7), recipients = {event.sender}}
    end
    return {allow = true, text = event.text}
end
