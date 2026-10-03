--!strict
return function(host: BloxStartupHost): ()
    host.register_model {
        key = 'sprout:model', asset = 'sprout:sprout', controls = 'sprout:looks', scale = 1,
    }
    host.register_creature {
        key = 'sprout:sproutling', module = 'sprout:creature', schema = 1, revision = 1,
        max_state_bytes = 8, initial_state = '0:0', interval = 10, read_radius = 1,
        body = {half_width = 0.36, height = 1.9, speed = 1}, interaction = 'pat',
        model = {key = 'sprout:model', idle = 'idle', walk = 'bounce', run = 'bounce'},
    }
end
