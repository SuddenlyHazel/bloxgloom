return function(c, e)
    if e.kind == "PlayerJoined" and e.player and not e.player.model then
        c.set_player_model(e.player.session, "rig:player")
        c.message_player(e.player.session, "Packaged rig selected. Character menu edits its looks; F4: rig:nod plays its baked nod clip.")
    end
    return nil
end
