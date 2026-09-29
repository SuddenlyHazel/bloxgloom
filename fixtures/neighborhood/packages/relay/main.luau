--!strict
return function(h: BloxStartupHost): ()
    h.register_system {
        key = 'relay:grow', schema = 1, revision = 1, module = 'relay:grow',
        max_state_bytes = 64, max_jobs_per_tick = 2,
        read_world = true, read_radius_chunks = 1,
        accepts_intents = true, intent_bootstrap = 'new',
        seeds = {{x=0, y=5, z=0, data='seed'}},
    }
end
