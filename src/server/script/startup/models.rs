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
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, value: Value| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error { return Err(error); }
            if !snapshot.permits_content(&namespace) { return Err("register_model requires content/v1"); }
            let Value::Table(table) = value else { return Err("model declaration must be a table"); };
            if table.metatable().is_some() { return Err("model declaration must be a plain table"); }
            for pair in table.clone().pairs::<Value,Value>().take(5) {
                let (key, _) = pair.map_err(|_| "invalid model declaration")?;
                if !matches!(key, Value::String(s) if [b"key".as_slice(), b"asset", b"controls", b"scale"].contains(&s.as_bytes().as_ref())) { return Err("unknown model declaration field"); }
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
            let definition = ModelAsset { key, glb: glb.to_vec(), controls, scale };
            definition.validate().map_err(|_| "invalid model declaration bounds")?;
            let prepared = crate::content::models::prepare(&definition).map_err(|_| "invalid model GLB or controls")?;
            pending.reserve_content(1, definition.glb.len() + definition.controls.len() + 512, &definition.key)?;
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

#[cfg(test)]
mod tests;
