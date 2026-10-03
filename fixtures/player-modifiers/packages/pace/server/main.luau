return function(h)
    h.register_action("pace:movement", 1, "Tune player movement", "empty", nil, "pace:movement", {
        permission="Admin", arguments={{kind="player"}, {kind="count", default=1}},
    })
end
