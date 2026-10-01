return function(h)
    h.register_texture('audio:machine', 'machine')
    h.register_block('audio:machine', 'Audio timer machine', 'audio:machine')
    -- One byte: 0 means idle; 1..100 counts processing pulses.
    h.register_entity('audio:timer', 1, 1, 1, 1)
    h.register_handler('audio:placed', 1, 'BlockPlaced', 'audio:machine', 'audio:machine')
    h.register_handler('audio:removed', 1, 'BlockRemoved', 'audio:machine', 'audio:machine')
    h.register_handler('audio:tick', 1, 'EntityTick', 'audio:timer', 'audio:machine')
    h.register_action('audio:start', 1, 'Start audio timer', 'block', 'audio:machine', 'audio:machine')
end
