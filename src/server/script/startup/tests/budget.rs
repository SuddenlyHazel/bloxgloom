use super::*;

fn state_options() -> &'static str {
    r#"
    local names = {'a','b','c','d','e','f','g','h'}
    local properties = {}
    for _,name in names do properties[name] = {'x','y'} end
    local states = {}
    for i=0,31 do
        local state = {}
        for j,name in names do
            state[name] = if j <= 5 and bit32.band(i, bit32.lshift(1,j-1)) ~= 0 then 'y' else 'x'
        end
        table.insert(states,state)
    end
    local options = {properties=properties, states=states}
    "#
}

#[test]
fn shared_state_options_cannot_bypass_host_memory_admission_through_pcall() {
    let fixture = Fixture::new();
    fixture.package("farm", "", &format!(
        "{}; for i=1,16 do h.register_texture('farm:t_'..i,'pixel') end; for i=1,200 do pcall(function() h.register_block('farm:crop_'..i,'Crop','farm:t_1',options) end) end",
        state_options()
    ));
    let path = fixture.0.join("farm/assets/textures/pixel.png");
    let mut png = fs::read(&path).unwrap();
    png.resize(crate::server::script::capacity::MAX_ASSET_BYTES, 0);
    fs::write(path, png).unwrap();
    let error = fixture.error();
    assert!(
        error.contains("register_block farm:crop_")
            && error.contains("estimated declaration bytes/package"),
        "{error}"
    );
}

#[test]
fn independent_packages_share_installation_metadata_admission_before_merge() {
    let fixture = Fixture::new();
    for name in ["a", "b"] {
        fixture.package(name, "", &format!(
            "{}; h.register_texture('{name}:tile','pixel'); for i=1,200 do h.register_block('{name}:crop_'..i,'Crop','{name}:tile',options) end",
            state_options()
        ));
    }
    let error = fixture.error();
    assert!(
        error.contains("b:package") && error.contains("estimated declaration bytes/installation"),
        "{error}"
    );
}
