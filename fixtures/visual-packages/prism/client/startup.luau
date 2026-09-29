return function(host)
    host.set_parameter("prism:surface", "tint", {0.2, 0.8, 1.0, 1.0})
    host.set_parameter("prism:mix", "strength", 0.35)
    host.set_replica_handler("prism:replica")
end
