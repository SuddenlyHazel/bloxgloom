//! V48 binds package-owned opaque GLB/control payloads to prepared model assets.
//! Decoding runs during verified artifact preparation, before session publication.
use super::*;
use crate::server::script::startup::models::PackageModel;
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x30";

pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.models.is_empty() {
        return Ok(bundle);
    }
    let mut models = declarations.models.iter().collect::<Vec<_>>();
    models.sort_by(|a, b| a.definition.key.cmp(&b.definition.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(models.len())?;
    for model in models {
        writer.field(model.definition.key.as_bytes())?;
        writer.field(model.asset.as_bytes())?;
        writer.field(model.controls_asset.as_deref().unwrap_or("").as_bytes())?;
        writer.field(&model.definition.scale.to_le_bytes())?;
    }
    ClientBundle::decode_verify(&writer.0, CacheKey(Sha256::digest(&writer.0).into()))
}

pub(in crate::server::script::package::client) fn decode(
    bytes: &[u8],
    expected: CacheKey,
) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.get(..8) == Some(b"BGCLIENT") && inner.get(8).is_some_and(|v| *v >= MAGIC[8]) {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(crate::content::models::MAX_MODELS)?;
    if count == 0 || !startup.models.is_empty() {
        return Err(invalid());
    }
    let mut previous = String::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for _ in 0..count {
        let key = reader.text(129)?;
        let (owner, local) = key.split_once(':').ok_or_else(invalid)?;
        if key <= previous || !identifier(owner) || !identifier(local) {
            return Err(invalid());
        }
        let package = startup
            .packages
            .iter()
            .find(|p| p.key == format!("{owner}:package"))
            .ok_or_else(invalid)?;
        if !package.requires.iter().any(|r| r == composition::CONTENT) {
            return Err(invalid());
        }
        let total = counts.entry(owner.to_owned()).or_default();
        *total += 1;
        if *total > 8 {
            return Err(invalid());
        }
        let asset = reader.text(129)?;
        let controls_asset = reader.text(129)?;
        let scale = f32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
        let payload = |name: &str, kind: u32| -> Result<Vec<u8>, ScriptError> {
            let (namespace, local) = name.split_once(':').ok_or_else(invalid)?;
            if namespace != owner || !identifier(local) {
                return Err(invalid());
            }
            let (tag, bytes) = bundle
                .packages
                .get(owner)
                .ok_or_else(invalid)?
                .model_assets
                .get(local)
                .ok_or_else(invalid)?;
            if *tag != kind {
                return Err(invalid());
            }
            Ok(bytes.clone())
        };
        let glb = payload(&asset, 11)?;
        let controls = if controls_asset.is_empty() {
            Vec::new()
        } else {
            payload(&controls_asset, 12)?
        };
        let definition = bloxgloom_host_api::model::ModelAsset {
            key: key.clone(),
            glb,
            controls,
            scale,
        };
        let prepared =
            crate::content::models::prepare(&definition).map_err(|e| error(owner, e.0))?;
        bundle.residency.add_payload(
            definition.glb.len() + definition.controls.len() + prepared.decoded_bytes(),
        )?;
        previous = key;
        startup.models.push(PackageModel {
            definition,
            asset,
            controls_asset: (!controls_asset.is_empty()).then_some(controls_asset),
            prepared,
        });
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    // Compile against the model catalog here to enforce the aggregate decoded
    // budget before publishing the artifact, rather than deferring to game join.
    let mut catalog = crate::content::Catalog::new();
    for model in &startup.models {
        catalog
            .register_prepared_model(model.definition.key.clone(), model.prepared.clone())
            .map_err(|e| error("<client-model>", e.0))?;
    }
    bundle.residency.resize(bytes.len())?;
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
