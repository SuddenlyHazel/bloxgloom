--!strict
return function(host: BloxStartupHost): ()
    host.register_action('uitarget:light', 1, 'Light aimed stone', 'block', 'bloxgloom:stone', 'uitarget:light')
end
