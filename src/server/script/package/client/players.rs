//! V42 wraps an unchanged canonical artifact with inert player-service identities.
//! Neither private initial state nor executable server callbacks reach clients.
use super::*;
use crate::content::client_metadata::Identity;

pub(super) const MAGIC: &[u8] = b"BGCLIENT\x2a";

pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.player_lifecycles.is_empty() {
        return Ok(bundle);
    }
    if declarations.player_lifecycles.len() > 128 {
        return Err(invalid());
    }
    let mut registrations = declarations.player_lifecycles.iter().collect::<Vec<_>>();
    registrations.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(registrations.len())?;
    for registration in registrations {
        writer.field(registration.key.as_bytes())?;
        let identity = Identity::new(
            b'Q',
            registration.key.clone(),
            &registration.fingerprint_bytes(),
        );
        writer.field(&identity.fingerprint.to_le_bytes())?;
    }
    let key = CacheKey(Sha256::digest(&writer.0).into());
    ClientBundle::decode_verify(&writer.0, key)
}

pub(super) fn decode(bytes: &[u8], expected: CacheKey) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.starts_with(super::declarations::anchored::MAGIC)
        || inner.starts_with(MAGIC)
        || inner.starts_with(super::observers::MAGIC)
    {
        return Err(invalid());
    }
    let mut bundle = ClientBundle::decode_verify(inner, CacheKey(Sha256::digest(inner).into()))?;
    let count = reader.count(128)?;
    if count == 0 {
        return Err(invalid());
    }
    let mut identities = Vec::with_capacity(count);
    let mut previous = String::new();
    let mut per_package = BTreeMap::<String, usize>::new();
    for _ in 0..count {
        let key = reader.text(128)?;
        let (namespace, local) = key.split_once(':').ok_or_else(invalid)?;
        if key <= previous || !identifier(local) {
            return Err(invalid());
        }
        bundle.packages.get(namespace).ok_or_else(invalid)?;
        // The startup metadata retains only public capability declarations.
        let startup = bundle.declarations.as_ref().ok_or_else(invalid)?;
        if !startup.permits_players(namespace) {
            return Err(invalid());
        }
        let own = per_package.entry(namespace.into()).or_default();
        *own += 1;
        if *own > 8 {
            return Err(invalid());
        }
        let fingerprint = u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
        previous = key.clone();
        identities.push(Identity {
            kind: b'Q',
            key,
            fingerprint,
        });
    }
    if !reader.0.is_empty() {
        return Err(invalid());
    }
    bundle
        .declarations
        .as_mut()
        .ok_or_else(invalid)?
        .set_player_identities(identities);
    // Retire the inner canonical buffer before allocating the outer one.
    // Nested compatibility wrappers must not add another full payload copy
    // to the download + canonical + decoded-payload reservation.
    bundle.residency.resize(bytes.len())?;
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
