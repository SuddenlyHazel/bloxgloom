return function(host)
    host.register_player_lifecycle("demo:players", 1, 32, "", "demo:players")
    host.register_region("demo:spawn", {-16, -256, -16}, {16, 512, 16}, "demo:players")
    host.register_chat_hook("demo:moderation", 1, "demo:chat")
end
