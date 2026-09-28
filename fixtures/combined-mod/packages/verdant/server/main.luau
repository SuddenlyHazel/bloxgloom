return function(host)
    host.register_texture('verdant:tile', 'jade')
    host.register_block('verdant:jade', 'Jade', 'verdant:tile')
    host.register_action('verdant:plant', 1, 'Plant jade in aimed stone', 'block', 'bloxgloom:stone', 'verdant:plant')
    host.register_system {
        key='verdant:growth', schema=1, revision=1, module='verdant:grow',
        max_state_bytes=8, max_jobs_per_tick=1, read_world=true,
        seeds={{x=0,y=5,z=0,data='0'}}
    }
end
