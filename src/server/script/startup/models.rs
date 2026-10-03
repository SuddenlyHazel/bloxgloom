//! Startup binding of package-owned model assets to namespaced frozen controls.
use super::*;
use bloxgloom_host_api::model::ModelAsset;

#[derive(Clone, Debug)]
pub(in crate::server::script) struct PackageModel {
    pub definition: ModelAsset,
    pub asset: String,
    pub controls_asset: Option<String>,
    pub prepared: Arc<crate::content::models::Prepared>,
}

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
    player_model: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, value: Value| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error { return Err(error); }
            if !snapshot.permits_content(&namespace) { return Err("register_model requires content/v1"); }
            let Value::Table(table) = value else { return Err("model declaration must be a table"); };
            if table.metatable().is_some() { return Err("model declaration must be a plain table"); }
            for pair in table.clone().pairs::<Value,Value>().take(12) {
                let (key, _) = pair.map_err(|_| "invalid model declaration")?;
                if !matches!(key, Value::String(s) if [b"key".as_slice(), b"asset", b"controls", b"scale", b"clips", b"crossfade_s", b"first_person_hide", b"first_person_offset"].contains(&s.as_bytes().as_ref())) { return Err("unknown model declaration field"); }
            }
            let field = |name| table.raw_get::<Value>(name).map_err(|_| "invalid model field");
            let key = owned(text(field("key")?)?, &namespace)?;
            if pending.models.len() >= 8 || pending.models.iter().any(|m| m.definition.key == key) { return Err("duplicate model or limit exceeded (8/package)"); }
            let asset = owned(text(field("asset")?)?, &namespace)?;
            let glb = snapshot.model_asset(&namespace, asset.split_once(':').unwrap().1, 11).ok_or("model requires declared package-owned GLB asset")?;
            let controls_asset = match field("controls")? { Value::Nil => None, value => Some(owned(text(value)?, &namespace)?) };
            let controls = match controls_asset.as_ref() {
                None => Vec::new(),
                Some(key) => snapshot.model_asset(&namespace, key.split_once(':').unwrap().1, 12).ok_or("controls require declared package-owned model-controls asset")?.to_vec(),
            };
            let scale = match field("scale")? {
                Value::Nil => 1.0,
                Value::Integer(n) => n as f32,
                Value::Number(n) if n.is_finite() => n as f32,
                _ => return Err("model scale must be finite"),
            };
            let player = if player_model { Some(player_settings(&table)?) } else {
                for name in ["clips", "crossfade_s", "first_person_hide", "first_person_offset"] { if !matches!(field(name)?, Value::Nil) { return Err("player settings require register_player_model"); } }
                None
            };
            let definition = ModelAsset { key, glb: glb.to_vec(), controls, scale, player };
            definition.validate().map_err(|_| "invalid model declaration bounds")?;
            let prepared = crate::content::models::prepare(&definition).map_err(|_| "invalid model GLB or controls")?;
            pending.reserve_content(1, definition.glb.len() + definition.controls.len() + definition.player.as_ref().map(|p| p.encode().unwrap().len()).unwrap_or(0) + 512, &definition.key)?;
            pending.models.push(PackageModel { definition, asset, controls_asset, prepared });
            Ok(())
        })();
        result.map_err(|error| pending.reject(error, "register_model"))
    })
}
fn owned(key: String, namespace: &str) -> Result<String, &'static str> {
    if key.split_once(':').is_none_or(|(owner, local)| {
        owner != namespace || !super::super::package::manifest::identifier(local)
    }) {
        Err("model identity and assets must belong to declaring package")
    } else {
        Ok(key)
    }
}

fn player_settings(
    table: &mlua::Table,
) -> Result<bloxgloom_host_api::model::PlayerModel, &'static str> {
    let mut player = bloxgloom_host_api::model::PlayerModel::default();
    if let Value::Table(clips) = table
        .raw_get::<Value>("clips")
        .map_err(|_| "invalid clip mappings")?
    {
        if clips.metatable().is_some() {
            return Err("clip mappings must be plain");
        }
        for pair in clips.clone().pairs::<Value, Value>().take(7) {
            let (name, _) = pair.map_err(|_| "invalid clip mappings")?;
            if !matches!(name, Value::String(s) if [b"idle".as_slice(), b"walk", b"run", b"crouch", b"tool_left", b"tool_right"].contains(&s.as_bytes().as_ref()))
            {
                return Err("unknown player clip mapping");
            }
        }
        for (name, field) in [
            ("idle", &mut player.idle),
            ("walk", &mut player.walk),
            ("run", &mut player.run),
            ("crouch", &mut player.crouch),
            ("tool_left", &mut player.tool_left),
            ("tool_right", &mut player.tool_right),
        ] {
            match clips
                .raw_get::<Value>(name)
                .map_err(|_| "invalid clip mapping")?
            {
                Value::Nil => {}
                v => *field = Some(text(v)?),
            }
        }
    } else if !matches!(
        table
            .raw_get::<Value>("clips")
            .map_err(|_| "invalid clips")?,
        Value::Nil
    ) {
        return Err("clips must be a plain mapping table");
    }
    match table
        .raw_get::<Value>("crossfade_s")
        .map_err(|_| "invalid crossfade")?
    {
        Value::Nil => {}
        Value::Integer(v) => player.crossfade_s = v as f32,
        Value::Number(v) => player.crossfade_s = v as f32,
        _ => return Err("crossfade_s must be numeric"),
    }
    match table
        .raw_get::<Value>("first_person_hide")
        .map_err(|_| "invalid first-person nodes")?
    {
        Value::Nil => {}
        Value::Table(nodes) if nodes.metatable().is_none() && nodes.raw_len() <= 32 => {
            for i in 1..=nodes.raw_len() {
                player.first_person_hide.push(text(
                    nodes
                        .raw_get::<Value>(i)
                        .map_err(|_| "invalid first-person node")?,
                )?);
            }
            if nodes.clone().pairs::<Value, Value>().take(33).count() != nodes.raw_len() {
                return Err("first_person_hide must be a dense list");
            }
        }
        _ => return Err("first_person_hide must be a bounded plain list"),
    }
    match table
        .raw_get::<Value>("first_person_offset")
        .map_err(|_| "invalid first-person offset")?
    {
        Value::Nil => {}
        Value::Table(offset)
            if offset.metatable().is_none()
                && offset.raw_len() == 3
                && offset.clone().pairs::<Value, Value>().take(4).count() == 3 =>
        {
            for (i, v) in player.first_person_offset.iter_mut().enumerate() {
                *v = match offset
                    .raw_get::<Value>(i + 1)
                    .map_err(|_| "invalid first-person offset")?
                {
                    Value::Integer(v) => v as f32,
                    Value::Number(v) => v as f32,
                    _ => return Err("first_person_offset needs three numbers"),
                };
            }
        }
        _ => return Err("first_person_offset needs three numbers"),
    }
    player
        .validate()
        .map_err(|_| "invalid player model settings")?;
    Ok(player)
}
#[cfg(test)]
mod tests;
