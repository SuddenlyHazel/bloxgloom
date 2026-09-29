return function(input)
    if input.event ~= "replica:entities" then return {} end
    -- Only a bounded public replica window drives this presentation value.
    -- Empty windows reset it; no gameplay state or GPU commands are writable.
    local strength = math.min(#input.entities / 16, 1) * 0.25 + 0.25
    return {{op = "parameter", resource = "prism:mix", name = "strength", value = strength}}
end
