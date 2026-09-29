return function(host)
    host.register_texture("prism:tile", "tile")
    host.register_item("prism:token", "Prism Token", "prism:tile")
    host.register_block("prism:stone", "Prism Stone", "prism:tile")
    host.register_creature {
        key="prism:glimmer", module="prism:creature", schema=1, revision=1,
        max_state_bytes=8, initial_state="idle", interval=10,
        body={half_width=0.25,height=0.6,speed=1.0},
        model={{min={-0.22,0.02,-0.22},max={0.22,0.58,0.22},color={0.2,0.8,1.0}}},
    }
end
