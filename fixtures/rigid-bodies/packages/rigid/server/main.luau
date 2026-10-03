return function(h)
    h.register_moving_entity {
        key="rigid:crate",module="rigid:body",schema=1,revision=1,
        max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=3000,
        body={half_extents={0.3,0.15,0.1},max_speed=16,max_acceleration=32,
            gravity_scale=1,response="bounce",restitution=0.35},
        physics={linear_damping=0.1,angular_damping=0.5,friction=0.7,max_angular_speed=4},
        model={{min={-0.3,-0.15,-0.1},max={0.3,0.15,0.1},color={0.3,0.7,0.9}}},
    }
    h.register_action("rigid:launch",1,"Launch spinning crate","empty",nil,"rigid:launch",{permission="Admin"})
end
