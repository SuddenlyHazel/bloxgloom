//! Compare all frozen runtime structure, including seeds absent from the catalog.
use super::*;

#[derive(PartialEq, Eq)]
pub(in crate::server) struct Contract {
    catalog: u64,
    creatures: Vec<(String, Vec<u8>)>,
    systems: Vec<SystemDescriptor>,
    codecs: Vec<(SystemId, u16, usize)>,
    seeds: Vec<(SystemId, OwnerKey, Vec<u8>)>,
    generators: Vec<(String, u32)>,
}

impl ServerStartup {
    pub(in crate::server) fn reload_contract(&self) -> io::Result<Contract> {
        Ok(Contract {
            catalog: self.catalog.fingerprint(),
            creatures: self
                .catalog
                .mobile_entities()
                .map(|(_, definition)| {
                    definition
                        .behavior
                        .encode(&definition.behavior.initial())
                        .map(|bytes| (definition.key.clone(), bytes))
                        .map_err(|e| io::Error::other(format!("{e:?}")))
                })
                .collect::<io::Result<_>>()?,
            systems: self
                .systems
                .iter()
                .map(|(descriptor, _)| descriptor.clone())
                .collect(),
            codecs: self
                .owner_codecs
                .iter()
                .map(|(id, codec)| (id.clone(), codec.codec_version, codec.max_bytes))
                .collect(),
            seeds: self
                .owners
                .iter()
                .map(|(id, owner, value)| {
                    self.owner_codecs[id]
                        .codec
                        .encode(value)
                        .map(|bytes| (id.clone(), *owner, bytes))
                        .map_err(|e| io::Error::other(format!("{e:?}")))
                })
                .collect::<io::Result<_>>()?,
            generators: self
                .generation
                .iter()
                .map(|g| (g.key.clone(), g.revision))
                .collect(),
        })
    }
}
