--!strict
return function(h: BloxClientStartupHost): ()
    h.set_replica_handler("throw:visuals")
end
