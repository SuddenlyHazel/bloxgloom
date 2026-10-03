return function(h)
    h.register_damage_policy("demo:damage", 1, "demo:damage")
    h.register_health_hook("demo:health", 1, "demo:hooks")
    for _, name in {"hurt", "heal", "kill", "health"} do
        h.register_action("demo:" .. name, 1, "Health: " .. name, "empty", nil, "demo:action", {
            permission = "Player",
            arguments = if name == "hurt" or name == "heal" then {{kind="count",default=25}} else {},
        })
    end
end
