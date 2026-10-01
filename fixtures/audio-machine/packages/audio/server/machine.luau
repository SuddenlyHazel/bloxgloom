local function voice(id) return 'timer-' .. tostring(id):gsub(':', '-') end
return function(h, e)
    if e.kind == 'BlockPlaced' then
        local p = e.cell
        h.spawn_entity('audio:timer', p[1]+0.5, p[2]+1.25, p[3]+0.5, string.char(1))
        h.sound{kind='play', voice='start', clip='audio:start', position={p[1]+0.5,p[2]+0.5,p[3]+0.5}}
    elseif e.kind == 'BlockRemoved' or e.kind == 'ActionRequested' then
        local p = e.cell
        assert(p ~= nil)
        for _, entity in h.nearby_entities(p[1]+0.5,p[2]+1.25,p[3]+0.5,0.25) do
            if entity.entity_type == 'audio:timer' then
                if e.kind == 'BlockRemoved' then
                    h.remove_entity(entity.id)
                else
                    assert(h.entity_state(entity.id):byte(1) == 0, 'Timer already running')
                    h.update_entity(entity.id,string.char(1))
                    h.schedule_entity(entity.id,1)
                    h.sound{kind='play', voice=voice(entity.id), clip='audio:start', entity=entity.id}
                end
            end
        end
    elseif e.kind == 'EntityTick' then
        local pulse = h.entity_state(e.entity):byte(1)
        if pulse >= 100 then
            h.update_entity(e.entity,string.char(0))
            h.schedule_entity(e.entity,nil)
            h.sound{kind='play', voice=voice(e.entity), clip='audio:complete', entity=e.entity}
        else
            h.update_entity(e.entity,string.char(pulse+1))
            h.schedule_entity(e.entity,5)
        end
    end
end
