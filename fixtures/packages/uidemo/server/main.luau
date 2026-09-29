--!strict
return function(host: BloxStartupHost): ()
    host.register_action('uidemo:store', 1, 'Store one stick', 'item', 'bloxgloom:stick', 'uidemo:store')
end
