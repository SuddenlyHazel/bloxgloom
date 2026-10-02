//! V47 wraps frozen item bitmap art; dynamic callbacks stay client-side only.
use super::*;
use bloxgloom_host_api::icon::ItemIcon;
pub(in crate::server::script::package::client) const MAGIC: &[u8] = b"BGCLIENT\x2f";
pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.icons.is_empty() {
        return Ok(bundle);
    }
    let mut icons = declarations.icons.iter().collect::<Vec<_>>();
    icons.sort_by(|a, b| a.item.cmp(&b.item));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(icons.len())?;
    for icon in icons {
        writer.field(icon.item.as_bytes())?;
        writer.count(icon.rows.len())?;
        for row in &icon.rows {
            writer.field(row.as_bytes())?;
        }
        writer.count(icon.palette.len())?;
        for (symbol, rgba) in &icon.palette {
            let mut bytes = vec![*symbol];
            for value in rgba {
                bytes.extend(value.to_le_bytes());
            }
            writer.field(&bytes)?;
        }
    }
    let key = CacheKey(Sha256::digest(&writer.0).into());
    ClientBundle::decode_verify(&writer.0, key)
}
pub(in crate::server::script::package::client) fn decode(
    bytes: &[u8],
    expected: CacheKey,
) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.get(..8) == Some(b"BGCLIENT")
        && inner.get(8).is_some_and(|version| *version >= MAGIC[8])
    {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let startup = bundle.declarations.as_mut().ok_or_else(invalid)?;
    let count = reader.count(MAX_PACKAGES * 32)?;
    if count == 0 || !startup.icons.is_empty() {
        return Err(invalid());
    }
    let mut previous = String::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for _ in 0..count {
        let item = reader.text(129)?;
        if item <= previous || !startup.items.iter().any(|i| i.key == item) {
            return Err(invalid());
        }
        previous = item.clone();
        let owner = item.split_once(':').ok_or_else(invalid)?.0;
        let package = startup
            .packages
            .iter()
            .find(|p| p.key == format!("{owner}:package"))
            .ok_or_else(invalid)?;
        if !package.requires.iter().any(|r| r == composition::CONTENT) {
            return Err(invalid());
        }
        let total = counts.entry(owner.into()).or_default();
        *total += 1;
        if *total > 32 {
            return Err(invalid());
        }
        let mut rows = Vec::new();
        for _ in 0..reader.count(32)? {
            rows.push(reader.text(32)?);
        }
        let mut palette = Vec::new();
        for _ in 0..reader.count(32)? {
            let bytes = reader.field(17)?;
            if bytes.len() != 17 {
                return Err(invalid());
            }
            let color = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[1 + i * 4..5 + i * 4].try_into().unwrap())
            });
            if palette
                .last()
                .is_some_and(|(symbol, _)| *symbol >= bytes[0])
            {
                return Err(invalid());
            }
            palette.push((bytes[0], color));
        }
        let icon = ItemIcon {
            item,
            rows,
            palette,
        };
        icon.validate().map_err(|_| invalid())?;
        startup.icons.push(icon);
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    bundle.residency.resize(bytes.len())?;
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
