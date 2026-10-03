return function(c,e)
    local p=c.player_position
    c.spawn_moving_entity("rigid:crate",{
        position={p[1]+2,p[2]+3,p[3]},velocity={2,1,0},angular_velocity={1,2,0},state="C",
    })
end
