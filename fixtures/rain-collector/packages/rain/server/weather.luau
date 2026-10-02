-- Advisory notification only. Storage decisions read captured weather in ticks.
return function(e)
    print('Weather target:',e.current.kind,'rain mm/h:',e.current.rain_mm_h)
end
