return function(host, event)
    if event.kind == "RegionEntered" then
        host.message_player(event.player.session, "Entered " .. event.region)
    elseif event.kind == "RegionLeft" and host.player_by_session(event.player.session) then
        host.message_player(event.player.session, "Left " .. event.region)
    end
end
