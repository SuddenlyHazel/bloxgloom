return function(h)
    h.register_action("pace:movement", 1, "Tune player movement", "empty", nil, "pace:movement", {
        permission="Admin", arguments={{kind="player"}, {kind="count", default=1}},
    })
    h.register_action("pace:self", 1, "Tune my movement", "empty", nil, "pace:movement", {
        permission="Admin", arguments={{kind="count", default=1}},
    })
end
