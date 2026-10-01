//! Frozen namespaced clips, decoded by bundle preparation workers.
use super::clip::Clip;
use std::{collections::BTreeMap, sync::Arc};
pub(crate) type Clips = BTreeMap<String, Arc<Clip>>;
pub(crate) fn prepare(
    packages: &BTreeMap<String, crate::server::client_bundle::ClientPackage>,
) -> Result<Clips, String> {
    let mut clips = builtin()?;
    for (owner, package) in packages {
        for (name, bytes) in &package.sound_assets {
            let key = format!("{owner}:{name}");
            if clips.len() >= 256 {
                return Err("sound clips/installation exceeds 256".into());
            }
            let clip = Clip::decode(bytes).map_err(|e| format!("{key}: {e}"))?;
            if clips.insert(key, Arc::new(clip)).is_some() {
                return Err("duplicate sound key".into());
            }
        }
    }
    Ok(clips)
}
pub(crate) fn builtin() -> Result<Clips, String> {
    let mut clips = Clips::new();
    for (key, bytes) in [
        (
            "bloxgloom:break",
            include_bytes!("../../assets/sounds/break.wav").as_slice(),
        ),
        (
            "bloxgloom:place",
            include_bytes!("../../assets/sounds/place.wav").as_slice(),
        ),
        (
            "bloxgloom:pickup",
            include_bytes!("../../assets/sounds/pickup.wav").as_slice(),
        ),
        (
            "bloxgloom:interact",
            include_bytes!("../../assets/sounds/interact.wav").as_slice(),
        ),
    ] {
        clips.insert(
            key.into(),
            Arc::new(Clip::decode(bytes).map_err(|e| e.to_string())?),
        );
    }
    Ok(clips)
}

pub(crate) fn builtin_cached() -> Result<Arc<Clips>, String> {
    static BUILTIN: std::sync::OnceLock<Result<Arc<Clips>, String>> = std::sync::OnceLock::new();
    BUILTIN.get_or_init(|| builtin().map(Arc::new)).clone()
}
