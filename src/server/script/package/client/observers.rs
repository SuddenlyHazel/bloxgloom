//! V43 wraps an unchanged canonical artifact with inert committed-observer identities.
//! Executable server callbacks and sources never reach clients.
use super::*;
use crate::content::client_metadata::Identity;

pub(super) const MAGIC: &[u8] = b"BGCLIENT\x2b";

pub(super) fn wrap(
    bundle: ClientBundle,
    declarations: &crate::server::script::startup::Declarations,
) -> Result<ClientBundle, ScriptError> {
    if declarations.observers.is_empty() {
        return Ok(bundle);
    }
    if declarations.observers.len() > 128 {
        return Err(invalid());
    }
    let mut registrations = declarations.observers.iter().collect::<Vec<_>>();
    registrations.sort_by(|a, b| a.key.cmp(&b.key));
    let mut writer = Writer(MAGIC.to_vec());
    writer.field(&bundle.bytes)?;
    writer.count(registrations.len())?;
    for registration in registrations {
        writer.field(registration.key.as_bytes())?;
        let identity = Identity::new(
            b'O',
            registration.key.clone(),
            &registration.version.to_le_bytes(),
        );
        writer.field(&identity.fingerprint.to_le_bytes())?;
    }
    let key = CacheKey(Sha256::digest(&writer.0).into());
    ClientBundle::decode_verify(&writer.0, key)
}

pub(super) fn decode(bytes: &[u8], expected: CacheKey) -> Result<ClientBundle, ScriptError> {
    let mut reader = Reader(&bytes[MAGIC.len()..]);
    let inner = reader.field(MAX_BUNDLE_BYTES)?;
    if inner.starts_with(super::declarations::anchored::MAGIC) || inner.starts_with(MAGIC) {
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
        if !startup.permits_observers(namespace) {
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
            kind: b'O',
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
        .set_observer_identities(identities);
    // Retire the inner canonical buffer before allocating the outer one.
    // Nested compatibility wrappers must not add another full payload copy
    // to the download + canonical + decoded-payload reservation.
    bundle.residency.resize(bytes.len())?;
    drop(std::mem::take(&mut bundle.bytes));
    bundle.bytes = bytes.to_vec();
    bundle.key = expected;
    Ok(bundle)
}
