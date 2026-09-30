--!strict
-- @module @bloxgloom
return function(h: BloxStartupHost)
    h.register_texture("counter:tile", "tile")
    h.register_block("counter:block", "COUNTER POST", "counter:tile")
    h.register_anchored {
        entity = "counter:post",
        block = "counter:block",
        module = "counter:behavior",
        schema_version = 1,
        max_state_bytes = 9,
        max_public_bytes = 5,
        interval = 20,
        placement_cost = 3,
        removal_refund = 2,
        footprint = {
            {offset = {0, 0, 0}, state = "counter:block"},
            {offset = {0, 1, 0}, state = "counter:block"},
        },
        observe = {{0, -1, 0}, {1, 0, 0}},
        interaction = "increment",
    }
end
