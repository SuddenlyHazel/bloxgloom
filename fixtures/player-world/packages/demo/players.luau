return function(host, event)
    if event.kind == "PlayerJoined" then
        host.message_player(event.player.session, "Region demo: T opens chat. Press F3 for coordinates; both X and Z must be between -16 and 16 to enter spawn.")
    elseif event.kind == "RegionEntered" then
        host.message_player(event.player.session, "Entered " .. event.region)
    elseif event.kind == "RegionLeft" and host.player_by_session(event.player.session) then
        host.message_player(event.player.session, "Left " .. event.region)
    end
end
