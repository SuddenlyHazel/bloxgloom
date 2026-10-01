#!/usr/bin/env python3
"""Rebuild the gameplay fixture deterministically; no external Python packages."""
from pathlib import Path
import json
import shutil
import struct
import zlib

ROOT = Path(__file__).resolve().parent
PACKAGE = ROOT / 'packages' / 'farm'
SPECIES = 'barley wheat rye oats corn rice millet sorghum pea bean lentil chickpea carrot beet turnip radish potato yam onion garlic leek cabbage kale lettuce spinach tomato pepper cucumber pumpkin melon strawberry blueberry'.split()
STAGES = ['seedling', 'young', 'flowering', 'ripe']
modules = []
assets = []

def module(name, source, side='server'):
    path = f'{side}/{name}.luau'
    target = PACKAGE / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('--!strict\n' + source + '\n')
    modules.append(f'module {side} {name} {path}')

def asset(kind, key, relative, data):
    target = PACKAGE / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data if isinstance(data, bytes) else data.encode())
    assets.append(f'asset {kind} {key} {relative}')

def png(color):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\0' + bytes(color) * 16 for _ in range(16))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 16, 16, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b'')

for i, species in enumerate(SPECIES):
    module('crop_' + species, 'return {key="' + species + '", title="' + species.title() + '", family=' + str(i % 4 + 1) + ', stages={"seedling","young","flowering","ripe"}}')
    module('recipe_' + species, 'return {seed="farm:' + species + '_seed", produce="farm:' + species + '_produce", planting_cost=1, harvest_yield=1}')
for i in range(22):
    module(f'calendar_{i:02}', f'return {{name="Season phase {i + 1}", duration={200 + 20 * i}, next_phase={(i + 1) % 22}}}')

main = '''return function(h: BloxStartupHost): ()
    for family = 1, 4 do
        h.register_texture("farm:leaf_" .. family, "leaf_" .. family)
        h.register_texture("farm:produce_" .. family, "produce_" .. family)
    end
'''
for species in SPECIES:
    main += f'''    do
        local crop = import("farm:crop_{species}")
        local recipe = import("farm:recipe_{species}")
        for _, stage in ipairs(crop.stages) do
            h.register_block("farm:" .. crop.key .. "_" .. stage, crop.title .. " " .. stage, "farm:leaf_" .. crop.family)
        end
        h.register_item(recipe.seed, crop.title .. " seeds", "farm:produce_" .. crop.family)
        h.register_item(recipe.produce, crop.title, "farm:produce_" .. crop.family)
    end
'''
main += '''    h.register_action("farm:plant", 1, "Plant barley: one seed", "block", "bloxgloom:stone", "farm:plant")
    h.register_action("farm:harvest", 1, "Harvest ripe barley", "block", "farm:barley_ripe", "farm:harvest")
    h.register_system {key="farm:irrigation", schema=1, revision=1, module="farm:irrigation", max_state_bytes=16, max_jobs_per_tick=1, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_system {key="farm:growth", schema=1, revision=1, module="farm:growth", max_state_bytes=16, max_jobs_per_tick=1, read_world=true, after={"farm:irrigation"}, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_system {key="farm:seasons", schema=1, revision=1, module="farm:seasons", max_state_bytes=16, max_jobs_per_tick=1, seeds={{x=0,y=5,z=0,data="0"}}}
    h.register_generator("farm:10_wild_crops", 1, "farm:wild_crops")
    h.register_generator("farm:20_groves", 1, "farm:groves")
end'''
module('main', main)
module('irrigation', '''return function(c: BloxOwnerContext): (string, number)
    c.wake("farm:growth", 0, 5, 0)
    return tostring((tonumber(c.data) or 0) + 1), 100
end''')
module('growth', '''return function(c: BloxOwnerContext): (string, number)
    -- A bounded demonstration bed in the seeded owner's authoritative chunk.
    for x=2, 5 do
        local before = c.block(x, 81, 0)
        for index, stage in ipairs({"seedling", "young", "flowering"}) do
            if before == "farm:barley_" .. stage then
                local after = ({"young", "flowering", "ripe"})[index]
                c.edit(x, 81, 0, before, "farm:barley_" .. after)
            end
        end
    end
    return tostring((tonumber(c.data) or 0) + 1), 1000
end''')
module('seasons', '''return function(c: BloxOwnerContext): (string, number)
    local phase = (tonumber(c.data) or 0) % 22
    local calendar = import("farm:calendar_" .. string.format("%02d", phase))
    return tostring(calendar.next_phase), calendar.duration
end''')
module('plant', '''local recipe = import("farm:recipe_barley")
return function(c: BloxGameplayContext, e: BloxActionEvent): ()
    assert(e.cell ~= nil)
    local selected = c.inventory("player")[e.slot + 1].stack
    assert(selected ~= nil and selected.item == recipe.seed, "Select barley seeds")
    assert(c.take("player", e.slot, recipe.planting_cost) ~= nil, "Need one barley seed")
    c.set_block(e.cell[1], e.cell[2], e.cell[3], "farm:barley_seedling")
end''')
module('harvest', '''local recipe = import("farm:recipe_barley")
return function(c: BloxGameplayContext, e: BloxActionEvent): ()
    assert(e.cell ~= nil)
    -- Both insertion and edit roll back if either fails; host stack cap is 128.
    assert(c.give("player", {item=recipe.produce,count=recipe.harvest_yield}), "Need inventory room")
    c.set_block(e.cell[1], e.cell[2], e.cell[3], "bloxgloom:stone")
end''')
module('wild_crops', '''return function(c: BloxGenerationContext): ()
    for x=0,15 do for z=0,15 do
        local wx, _, wz = c.world_position(x,0,z)
        local y = c.builtin_terrain_height(wx,wz) + 1
        local _, base, _ = c.world_position(0,0,0)
        local ly = y - base
        if ly >= 0 and ly < 16 and c.random_at(wx,y,wz,41) < 0.025 then
            c.set_block(x,ly,z,"farm:barley_seedling")
        end
    end end
end''')
module('groves', '''return function(c: BloxGenerationContext): ()
    -- Absolute grid decisions also agree for negative chunks and seams.
    for x=0,15 do for z=0,15 do
        local wx, base, wz = c.world_position(x,0,z)
        local y = c.builtin_terrain_height(wx,wz) + 1
        local ly = y - base
        if ly >= 0 and ly < 16 and wx % 8 == 0 and wz % 8 == 0 then
            c.set_block(x,ly,z,"farm:blueberry_ripe")
        end
    end end
end''')
module('view', '''return function(input: BloxUiInput): {BloxUiCommand}
    if input.event == "farm:plant" or input.event == "farm:harvest" then
        return {{op="action",key=tostring(input.event)}}
    end
    return {}
end''', 'client')
module('client_startup', '''return function(h: BloxClientStartupHost): ()
    h.set_text("farm:controls/title", "Farming: 32 crops, independent growth and seasons")
end''', 'client')
for family, color in enumerate([(86,140,58,255),(127,168,64,255),(186,156,60,255),(90,133,116,255)],1):
    asset('texture', f'leaf_{family}', f'assets/textures/leaf_{family}.png', png(color))
    asset('texture', f'produce_{family}', f'assets/textures/produce_{family}.png', png(tuple(min(255,c+25) for c in color[:3])+(255,)))
asset('ui-document','controls','assets/ui/controls.json',json.dumps({'version':1,'presentation':{'capability':'local-ui','module':'farm:view'},'nodes':[{'id':'root','kind':'panel','style':'farm:panel'},{'id':'title','parent':0,'kind':'label','style':'farm:button','text':'Farming'},{'id':'plant','parent':0,'kind':'button','style':'farm:button','text':'Plant barley: one selected seed','event':'farm:plant'},{'id':'harvest','parent':0,'kind':'button','style':'farm:button','text':'Harvest ripe barley: one produce','event':'farm:harvest'}]}))
asset('ui-style','panel','assets/ui/panel.json',json.dumps({'width':600,'height':250,'padding':12,'gap':8,'background':[24,36,32,245]}))
asset('ui-style','button','assets/ui/button.json',json.dumps({'width':576,'height':56,'padding':8,'background':[48,86,65,255],'color':[240,250,235,255],'font':'farm:body'}))
asset('ui-font','body','assets/fonts/body.ttf',(ROOT.parent/'combined-mod/packages/verdant/assets/fonts/body.ttf').read_bytes())
shutil.copyfile(ROOT.parent/'combined-mod/packages/verdant/assets/fonts/OFL.txt', PACKAGE/'assets/fonts/OFL.txt')
assert len(modules) == 96
manifest = 'format 2\npackage farm\nversion 1.0.0\nentry main\n'
manifest += ''.join('requires bloxgloom:'+cap+'/v1\n' for cap in ['content','actions','owner_systems','generation'])
(PACKAGE/'package.txt').write_text(manifest+'\n'.join(modules+assets)+'\n')
print(f'Wrote {len(modules)} modules, 128 blocks, 192 items, {len(assets)} assets')
