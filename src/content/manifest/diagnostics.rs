//! Errors grounded in the persisted BGCM identity evidence. Fingerprints do
//! not retain individual fields, so never pretend to know which field changed.

use std::io;

use super::ContentEntry;

pub(super) fn saved_contract(current: &ContentEntry, id: u32, saved: u64) -> io::Error {
    invalid(format!(
        "incompatible saved content: {} '{}' (namespace '{}', saved ID {id}): persisted contract fingerprint changed; saved {saved:016x}, current {:016x}. The save records the contract hash, not individual fields; restore the compatible definition or use a new world directory (existing save left unchanged)",
        kind(current.kind),
        current.key,
        namespace(&current.key),
        current.schema_fingerprint,
    ))
}

pub(super) fn missing_client(entry: &ContentEntry) -> io::Error {
    invalid(format!(
        "missing client content definition: {} '{}' (namespace '{}', server ID {}, server contract {:016x}); install the server's package revision",
        kind(entry.kind),
        entry.key,
        namespace(&entry.key),
        entry.id,
        entry.schema_fingerprint,
    ))
}

pub(super) fn extra_client(entry: &ContentEntry) -> io::Error {
    invalid(format!(
        "extra client content definition: {} '{}' (namespace '{}', client contract {:016x}); this key is absent from the server catalog; install the server's package revision",
        kind(entry.kind),
        entry.key,
        namespace(&entry.key),
        entry.schema_fingerprint,
    ))
}

pub(super) fn client_contract(server: &ContentEntry, client: &ContentEntry) -> io::Error {
    invalid(format!(
        "client content contract differs: {} '{}' (namespace '{}', server ID {}): server {:016x}, client {:016x}; install the server's package revision",
        kind(server.kind),
        server.key,
        namespace(&server.key),
        server.id,
        server.schema_fingerprint,
        client.schema_fingerprint,
    ))
}

fn namespace(key: &str) -> &str {
    key.split_once(':').map_or(key, |(namespace, _)| namespace)
}

fn kind(kind: u8) -> &'static str {
    match kind {
        b'B' => "block",
        b'S' => "block state",
        b'I' => "item",
        b'E' => "entity",
        b'P' => "package",
        b'T' => "block tag",
        b'U' => "item tag",
        b'Y' => "owner system",
        b'G' => "gameplay handler",
        b'O' => "gameplay observer",
        b'Q' => "player lifecycle",
        b'M' => "model",
        _ => "content",
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
