return function(input)
    if input.event~='replica:entities' then return {} end
    local commands={}
    for _,entity in input.entities do
        if #commands>=16 then break end
        if entity.key=='rain:collector_state' then
            local voice='collector-loop-'..tostring(entity.id):gsub(':','-')
            local fill,active=entity.public:byte(1,2)
            if active==1 then
                commands[#commands+1]={op='sound',kind='play',voice=voice,clip='rain:motor',entity=entity.id,looping=true,gain=0.35,pitch=0.6+fill/200}
            else commands[#commands+1]={op='sound',kind='stop',voice=voice} end
        end
    end
    return commands
end
