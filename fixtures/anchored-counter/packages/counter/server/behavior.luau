--!strict
-- @module @bloxgloom
-- Private: little-endian u32 count, u8 neighbor signal, i32 original height.
-- Public: count and signal only; the original height remains server-private.
local function valid(state: string): boolean
    return #state == 9 and string.byte(state, 5) <= 1
end

return function(e: BloxAnchoredEvent): BloxAnchoredReply
    if e.kind == "Initialize" then
        local state = buffer.create(9)
        buffer.writei32(state, 5, e.anchor[2])
        return buffer.tostring(state)
    elseif e.kind == "Validate" then
        return valid(e.state)
    end

    assert(valid(e.state), "invalid counter state")
    if e.kind == "Public" then
        return string.sub(e.state, 1, 5)
    elseif e.kind == "Interact" then
        assert(e.request == "increment", "unknown counter request")
        local state = buffer.fromstring(e.state)
        local count = buffer.readu32(state, 0)
        assert(count < 4294967295, "counter exhausted")
        buffer.writeu32(state, 0, count + 1)
        return buffer.tostring(state)
    elseif e.kind == "React" then
        local support = nil
        local neighbor = nil
        for _, cell in ipairs(e.cells) do
            if cell.offset[1] == 0 and cell.offset[2] == -1 and cell.offset[3] == 0 then
                support = cell.solid
            elseif cell.offset[1] == 1 and cell.offset[2] == 0 and cell.offset[3] == 0 then
                neighbor = cell.solid
            end
        end
        assert(support ~= nil and neighbor ~= nil, "missing observed terrain")
        if not support then
            return {remove = true}
        end
        local signal = if neighbor then 1 else 0
        if string.byte(e.state, 5) == signal then
            return nil
        end
        local state = buffer.fromstring(e.state)
        buffer.writeu8(state, 4, signal)
        return {state = buffer.tostring(state)}
    elseif e.kind == "Refund" then
        assert(e.cause == "Broken" or e.cause == "Reaction" or e.cause == "WorldEdit")
        return math.min(e.maximum, 2)
    end
    error("unknown counter event")
end
