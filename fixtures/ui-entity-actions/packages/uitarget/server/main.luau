return function(host)
    host.register_creature {
        key = 'uitarget:marker', module = 'uitarget:creature', schema = 1,
        revision = 1, max_state_bytes = 8, initial_state = 'new', interval = 1000,
        body = {half_width = 0.25, height = 0.7, speed = 0},
        interaction = 'pat',
        model = {{min={-0.2,0,-0.2}, max={0.2,0.7,0.2}, color={0.2,0.8,0.3}}},
    }
    host.register_action('uitarget:pat', 1, 'Pat marker', 'entity', 'uitarget:marker', 'uitarget:pat')
end
