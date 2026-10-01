return function(input)
    if input.event ~= 'replica:entities' then return {} end
    local commands = {}
    for _, entity in input.entities do
        if #commands >= 16 then break end
        if entity.key == 'audio:timer' then
            local voice = 'motor-' .. tostring(entity.id):gsub(':', '-')
            if entity.public:byte(1) > 0 then
                commands[#commands+1] = {op='sound', kind='play', voice=voice,
                    clip='audio:motor', entity=entity.id, looping=true, gain=0.45, pitch=0.8}
            else
                commands[#commands+1] = {op='sound', kind='stop', voice=voice}
            end
        end
    end
    return commands
end
