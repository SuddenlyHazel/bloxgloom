--!strict
return function(host: BloxClientStartupHost): ()
    local bytes = buffer.create(4)
    buffer.writeu32(bytes, 0, 1)
    log.debug('Garden presentation prepared', {version=buffer.readu32(bytes, 0), sample=math.random()})
    host.set_text('verdant:controls/title', 'Jade garden (downloaded mod)')
end
