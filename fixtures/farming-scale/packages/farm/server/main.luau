--!strict
return function(h: BloxStartupHost): ()
    for family = 1, 4 do
        h.register_texture("farm:leaf_" .. family, "leaf_" .. family)
        h.register_texture("farm:produce_" .. family, "produce_" .. family)
    end
    do
        local crop = import("farm:crop_barley")
        local recipe = import("farm:recipe_barley")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_wheat")
        local recipe = import("farm:recipe_wheat")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_rye")
        local recipe = import("farm:recipe_rye")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_oats")
        local recipe = import("farm:recipe_oats")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_corn")
        local recipe = import("farm:recipe_corn")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_rice")
        local recipe = import("farm:recipe_rice")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_millet")
        local recipe = import("farm:recipe_millet")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_sorghum")
        local recipe = import("farm:recipe_sorghum")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_pea")
        local recipe = import("farm:recipe_pea")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_bean")
        local recipe = import("farm:recipe_bean")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_lentil")
        local recipe = import("farm:recipe_lentil")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_chickpea")
        local recipe = import("farm:recipe_chickpea")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_carrot")
        local recipe = import("farm:recipe_carrot")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_beet")
        local recipe = import("farm:recipe_beet")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_turnip")
        local recipe = import("farm:recipe_turnip")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_radish")
        local recipe = import("farm:recipe_radish")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_potato")
        local recipe = import("farm:recipe_potato")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_yam")
        local recipe = import("farm:recipe_yam")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_onion")
        local recipe = import("farm:recipe_onion")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_garlic")
        local recipe = import("farm:recipe_garlic")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_leek")
        local recipe = import("farm:recipe_leek")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_cabbage")
        local recipe = import("farm:recipe_cabbage")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_kale")
        local recipe = import("farm:recipe_kale")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_lettuce")
        local recipe = import("farm:recipe_lettuce")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_spinach")
        local recipe = import("farm:recipe_spinach")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_tomato")
        local recipe = import("farm:recipe_tomato")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_pepper")
        local recipe = import("farm:recipe_pepper")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_cucumber")
        local recipe = import("farm:recipe_cucumber")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_pumpkin")
        local recipe = import("farm:recipe_pumpkin")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_melon")
        local recipe = import("farm:recipe_melon")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_strawberry")
        local recipe = import("farm:recipe_strawberry")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    do
        local crop = import("farm:crop_blueberry")
        local recipe = import("farm:recipe_blueberry")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
    h.register_action("farm:plant", 1, "Plant barley: one seed", "block", "bloxgloom:stone", "farm:plant")
    h.register_action("farm:harvest", 1, "Harvest ripe barley", "block", "farm:barley_ripe", "farm:harvest")
    h.register_system {key="farm:irrigation", schema=1, revision=1, module="farm:irrigation", max_state_bytes=16, max_jobs_per_tick=1, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_system {key="farm:growth", schema=1, revision=1, module="farm:growth", max_state_bytes=16, max_jobs_per_tick=1, read_world=true, after={"farm:irrigation"}, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_system {key="farm:seasons", schema=1, revision=1, module="farm:seasons", max_state_bytes=16, max_jobs_per_tick=1, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_generator("farm:10_wild_crops", 1, "farm:wild_crops")
    h.register_generator("farm:20_groves", 1, "farm:groves")
end
