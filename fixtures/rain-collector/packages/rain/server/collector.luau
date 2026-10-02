local function voice(id) return 'collector-'..tostring(id):gsub(':','-') end
return function(h,e)
    if e.kind=='BlockPlaced' then
        local p=e.cell
        h.spawn_entity('rain:collector_state',p[1]+0.5,p[2]+1.25,p[3]+0.5,string.char(0,0))
    elseif e.kind=='BlockRemoved' or e.kind=='ActionRequested' then
        local p=e.cell
        for _,entity in h.nearby_entities(p[1]+0.5,p[2]+1.25,p[3]+0.5,0.25) do
            if entity.entity_type=='rain:collector_state' then
                if e.kind=='BlockRemoved' then h.remove_entity(entity.id)
                else
                    local fill=h.entity_state(entity.id):byte(1)
                    assert(fill>0,'Collector is empty')
                    h.update_entity(entity.id,string.char(0,0))
                    h.schedule_entity(entity.id,50)
                    h.sound{kind='play',voice=voice(entity.id),clip='rain:complete',entity=entity.id}
                end
            end
        end
    elseif e.kind=='EntityTick' then
        local fill=h.entity_state(e.entity):byte(1)
        local weather=h.weather()
        local active=fill<100 and weather.rain_mm_h>0.1
        if active then
            local p=e.position
            local x,y,z=math.floor(p[1]),math.floor(p[2]),math.floor(p[3])
            -- This fixture's catchment clearance is 64 blocks above the mouth.
            -- Unavailable terrain defers the decision rather than inventing air.
            for above=y,y+63 do
                local block=h.block(x,above,z)
                if block.state~='bloxgloom:air' and not block.plant then active=false; break end
            end
        end
        if active then fill=math.min(100,fill+math.max(1,math.floor(weather.rain_mm_h/18+0.5))) end
        h.update_entity(e.entity,string.char(fill,if active and fill<100 then 1 else 0))
        h.schedule_entity(e.entity,50)
    end
end
