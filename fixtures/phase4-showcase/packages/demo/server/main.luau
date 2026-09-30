--!strict
return function(host: BloxStartupHost): ()
    host.register_texture('demo:press_idle', 'press_idle')
    host.register_texture('demo:press_lit', 'press_lit')
    local pressStates: {BloxBlockState} = {
            { lit = 'off' },
            { lit = 'on', emission = 12, textures = {
                top = 'demo:press_lit',
                side = 'demo:press_lit',
                bottom = 'demo:press_idle',
            } },
        }
    host.register_block('demo:press', 'Press', 'demo:press_idle', {
        properties = { lit = {'off', 'on'} },
        states = pressStates,
    })
    host.register_machine {
        entity = 'demo:press_machine', block = 'demo:press',
        module = 'demo:machine', schema = 1, revision = 1, interval = 50,
        title = 'STONE PRESS', hint = 'FUEL + STONE TO GRAVEL',
        footprint = {{0,0,0}, {1,0,0}},
        active_state = 'demo:press[lit=on]',
        recipe = {
            key = 'demo:crush', input = 'bloxgloom:stone', input_count = 1,
            output = 'bloxgloom:gravel', output_count = 2, pulses = 12,
        },
        fuel = {item = 'bloxgloom:stick', pulses = 30},
    }
    host.register_creature {
        key = 'demo:sproutling', module = 'demo:creature', schema = 1,
        revision = 1, max_state_bytes = 16, initial_state = 'new', interval = 10,
        body = {half_width = 0.28, height = 0.8, speed = 1.0},
        interaction = 'pat',
        animation = {idle_bob = 0.04, stride_rate = 6.0},
        model = {
            {min={-0.25,0.16,-0.24}, max={0.25,0.65,0.24}, color={0.18,0.72,0.30}},
            {min={-0.19,0.60,-0.12}, max={-0.08,0.90,0.12}, color={0.32,0.85,0.35}},
            {min={0.08,0.60,-0.12}, max={0.19,0.90,0.12}, color={0.32,0.85,0.35}},
            {min={-0.19,0.02,-0.16}, max={-0.05,0.24,0.12}, color={0.11,0.46,0.17}, motion='left_foot'},
            {min={0.05,0.02,-0.16}, max={0.19,0.24,0.12}, color={0.11,0.46,0.17}, motion='right_foot'},
        },
    }
end
