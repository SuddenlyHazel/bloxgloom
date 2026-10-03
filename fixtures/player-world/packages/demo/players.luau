local function chunk_key(position)
    return string.format("%d,%d,%d", math.floor(position[1] / 16), math.floor(position[2] / 16), math.floor(position[3] / 16))
end

return function(host, event)
    if event.kind == "PlayerJoined" then
        local chunk = chunk_key(event.player.position)
        host.message_player(event.player.session, "Region demo: walk or fly anywhere for chunk notices. T opens chat; F3 shows XYZ and chunk coordinates.")
        host.message_player(event.player.session, "Entered chunk " .. chunk)
        return {session_state = chunk, session_delay = 5}
    elseif event.kind == "SessionTick" then
        local player = host.player_by_session(event.player.session)
        if not player then return end
        local chunk = chunk_key(player.position)
        if event.session_state ~= "" and event.session_state ~= chunk then
            host.message_player(player.session, "Left chunk " .. event.session_state .. "; entered chunk " .. chunk)
        end
        return {session_state = chunk, session_delay = 5}
    elseif event.kind == "RegionEntered" then
        host.message_player(event.player.session, "Entered " .. event.region)
    elseif event.kind == "RegionLeft" and host.player_by_session(event.player.session) then
        host.message_player(event.player.session, "Left " .. event.region)
    end
end
