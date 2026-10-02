return function(host)
    host.register_item('visual:cell', 'Charge cell', 'bloxgloom:stone', {
        components = {version = 1, fingerprint = '123456789abcdef0', max_bytes = 8},
    })
    host.register_item_icon('visual:cell', {
        rows = {'..xx..', '.xxxx.', '.x..x.', '.x..x.', '.xxxx.'},
        palette = {x = {0.85, 0.75, 0.25, 1}},
    })
end
