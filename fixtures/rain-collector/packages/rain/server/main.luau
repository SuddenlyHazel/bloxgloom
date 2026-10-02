return function(h)
    h.register_texture('rain:collector', 'collector')
    h.register_block('rain:collector', 'Rain collector', 'rain:collector', {
        acoustics={surface='metal', habitat='none', impact={
            gain=0.8, click=0.55, frequency_hz={450,1100},
            damping_per_s={180,350}, resonance=0.65, lowpass_hz=5000,
        }},
    })
    h.register_block('rain:glass', 'Glass acoustic panel', 'rain:collector', {acoustics={surface='glass'}})
    h.register_block('rain:wood', 'Wood acoustic panel', 'rain:collector', {acoustics={surface='wood'}})
    h.register_block('rain:habitat', 'Insect habitat patch', 'rain:collector', {acoustics={surface='leaf',habitat='canopy'}})
    -- Two public bytes: stored fill (0..100), currently collecting (0/1).
    h.register_entity('rain:collector_state',1,2,2,1)
    for _, event in {'BlockPlaced','BlockRemoved'} do
        h.register_handler('rain:'..event:lower(),1,event,'rain:collector','rain:collector')
    end
    h.register_handler('rain:tick',1,'EntityTick','rain:collector_state','rain:collector')
    h.register_action('rain:empty',1,'Empty rain collector','block','rain:collector','rain:collector')
    h.register_weather_observer('rain:weather_notice',1,'rain:weather')
end
