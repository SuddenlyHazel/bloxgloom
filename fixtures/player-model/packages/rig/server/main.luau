return function(h)
    h.register_player_model {
        key = "rig:player", asset = "rig:player", controls = "rig:looks", scale = 1.0,
        clips = {idle = "idle", walk = "bounce", run = "bounce", crouch = "nod", tool_left = "nod", tool_right = "nod"},
        crossfade_s = 0.15, first_person_hide = {"eyes_classic", "eyes_sleepy"},
    }
    h.register_player_lifecycle("rig:welcome", 1, 16, "", "rig:joined")
    h.register_action("rig:nod", 1, "Nod player model", "empty", nil, "rig:nod", {permission = "Player"})
end
