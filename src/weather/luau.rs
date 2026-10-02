//! Shared readonly weather view for gameplay, observers and client presentation.
use bloxgloom_host_api::gameplay::Weather;
use mlua::{Lua, Table};
pub(crate) fn present(lua: &Lua, weather: Weather) -> mlua::Result<Table> {
    let value = lua.create_table()?;
    value.raw_set("kind", weather.kind.name())?;
    for (key, number) in [
        ("rain_mm_h", weather.rain_mm_h),
        ("wind_m_s", weather.wind_m_s),
        ("cloud", weather.cloud),
        ("transition", weather.transition),
    ] {
        value.raw_set(key, number)?;
    }
    for (key, number) in [
        ("revision", weather.revision),
        ("elapsed_ms", weather.elapsed_ms),
    ] {
        value.raw_set(format!("{key}_lo"), number as u32)?;
        value.raw_set(format!("{key}_hi"), (number >> 32) as u32)?;
    }
    value.set_readonly(true);
    Ok(value)
}
