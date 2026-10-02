return function(stack)
    local color = stack.count >= 64 and {0.2, 0.9, 0.3, 1} or {0.95, 0.55, 0.15, 1}
    if string.byte(stack.components, 1) == 255 then color = {0.2, 0.5, 1, 1} end
    return {
        icon = {
            rows = {'..xx..', '.xxxx.', '.x++x.', '.x++x.', '.xxxx.'},
            palette = {x = {0.75, 0.8, 0.85, 1}, ['+'] = color},
        },
        drop_scale = 0.5 + stack.count / 128,
    }
end
