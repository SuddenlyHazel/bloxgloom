--!strict
return function(h: BloxStartupHost): ()
    h.register_item("throw:seed", "Throwing seed", "bloxgloom:stone")
    h.register_moving_entity({
        key = "throw:seed_body", module = "throw:seed", schema = 1, revision = 1,
        max_state_bytes = 1, max_public_bytes = 1, interval = 10, lifetime_ticks = 500,
        handles_impact = true, handles_expiry = true,
        body = {origin = "center", half_extents = {0.12, 0.12, 0.12},
            collisions = {terrain = true}, response = "bounce", restitution = 0.6,
            gravity_scale = 1, max_speed = 24, max_acceleration = 32},
        model = {{min = {-0.12, -0.12, -0.12}, max = {0.12, 0.12, 0.12}, color = {0.3, 0.8, 0.2}}},
    })
    h.register_moving_entity({
        key = "throw:guided_body", module = "throw:guided", schema = 1, revision = 1,
        max_state_bytes = 1, max_public_bytes = 1, interval = 10, lifetime_ticks = 200,
        handles_impact = true, handles_expiry = true,
        body = {origin = "center", half_extents = {0.12, 0.12, 0.12},
            collisions = {terrain = true}, response = "bounce", restitution = 0.8,
            gravity_scale = 0, max_speed = 16, max_acceleration = 16},
        model = {{min = {-0.2, -0.08, -0.08}, max = {0.2, 0.08, 0.08}, color = {0.2, 0.5, 1.0}}},
    })
    h.register_action("throw:launch_seed", 1, "Throw seed", "item", "throw:seed", "throw:launch")
    h.register_action("throw:launch_guided", 1, "Launch guided seed", "item", "throw:seed", "throw:launch")
end
