//! A closed, data-only registration host. Never loads source, image payloads,
//! callbacks, paths or a VM. Public package requirements are identity metadata,
//! not permissions granted to the client. Only startup's fixed sprite shape is
//! representable; other catalog declarations are explicitly unsupported.
use super::*;
use bloxgloom_host_api::{composition, content};

const MAX_ITEMS: usize = 32;

#[derive(Debug)]
pub(super) struct Startup {
    packages: Vec<composition::Package>,
    items: Vec<content::Item>,
    unsupported: bool,
}

impl ClientBundle {
    /// Called only after every authoritative startup entry succeeds. Discovery's
    /// final absent-metadata marker is replaced; source/asset bytes stay intact.
    pub(in crate::server::script) fn with_startup(
        &self,
        packages: &[composition::Package],
        items: &[content::Item],
        unsupported: bool,
    ) -> Result<Self, ScriptError> {
        if self.declarations.is_some()
            || packages.len() != self.packages.len()
            || items.len() > MAX_PACKAGES * MAX_ITEMS
        {
            return Err(invalid());
        }
        let mut writer = Writer(self.bytes[..self.bytes.len() - 4].to_vec());
        writer.count(1)?;
        writer.count(usize::from(unsupported))?;
        let mut items = items.iter().collect::<Vec<_>>();
        items.sort_by(|a, b| a.key.cmp(&b.key));
        for ((name, _), package) in self.packages.iter().zip(packages) {
            if package.key != format!("{name}:package") || package.version != 1 {
                return Err(invalid());
            }
            writer.count(package.requires.len())?;
            for requirement in &package.requires {
                writer.field(requirement.as_bytes())?;
            }
            let own = items
                .iter()
                .filter(|item| {
                    item.key
                        .split_once(':')
                        .is_some_and(|(owner, _)| owner == name)
                })
                .collect::<Vec<_>>();
            writer.count(own.len())?;
            for item in own {
                if item.swatch != [1.0; 4]
                    || item.placeable.is_some()
                    || !item.sprite
                    || item.components != content::Components::None
                {
                    return Err(invalid());
                }
                writer.field(item.key.as_bytes())?;
                writer.field(item.name.as_bytes())?;
                writer.field(item.texture.as_bytes())?;
            }
        }
        let key = CacheKey(Sha256::digest(&writer.0).into());
        let result = Self::decode_verify(&writer.0, key)?;
        if result.declarations.as_ref().unwrap().items.len() != items.len() {
            return Err(invalid());
        }
        Ok(result)
    }

    /// Fresh session definitions, never installed in the process-global catalog.
    /// The caller must still resolve the exact server manifest and fingerprint.
    pub(crate) fn session_catalog(&self) -> std::io::Result<crate::content::Catalog> {
        let startup = self.declarations.as_ref().ok_or_else(|| {
            std::io::Error::other("client bundle has no startup registration metadata")
        })?;
        if startup.unsupported {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unsupported client catalog declarations (only startup sprite items are supported)",
            ));
        }
        let mut declarations = crate::content::declarations::Declarations::default();
        let mut compile = || -> Result<_, bloxgloom_host_api::RegistrationError> {
            for package in &startup.packages {
                declarations.package(package.clone())?;
            }
            for item in &startup.items {
                declarations.item(item.clone())?;
            }
            let mut catalog = crate::content::Catalog::builtins();
            declarations.install_base(&mut catalog)?;
            declarations.install_items_and_tags(&mut catalog)?;
            catalog.refresh_builtin_fuels()?;
            Ok(catalog)
        };
        compile().map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }
}

impl Startup {
    pub(super) fn decode(
        reader: &mut Reader<'_>,
        packages: &BTreeMap<String, ClientPackage>,
    ) -> Result<Option<Self>, ScriptError> {
        if reader.count(1)? == 0 {
            return Ok(None);
        }
        let mut startup = Self {
            packages: Vec::new(),
            items: Vec::new(),
            unsupported: reader.count(1)? != 0,
        };
        for (name, package) in packages {
            if name == "bloxgloom" {
                return Err(invalid());
            }
            let mut requires = Vec::new();
            for _ in 0..reader.count(4)? {
                let requirement = reader.text(64)?;
                if ![
                    composition::CONTENT,
                    composition::GENERATION,
                    composition::ACTIONS,
                    composition::OWNER_SYSTEMS,
                ]
                .contains(&requirement.as_str())
                    || requires.last().is_some_and(|last| last >= &requirement)
                {
                    return Err(invalid());
                }
                requires.push(requirement);
            }
            let count = reader.count(MAX_ITEMS)?;
            if count > 0 && !requires.iter().any(|r| r == composition::CONTENT) {
                return Err(invalid());
            }
            let mut previous = String::new();
            for _ in 0..count {
                let key = reader.text(129)?;
                let display = reader.text(255)?;
                let texture = reader.text(255)?;
                if key <= previous
                    || key
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != name || !identifier(local))
                    || display.is_empty()
                    || texture
                        .split_once(':')
                        .is_none_or(|(owner, local)| owner != "bloxgloom" || !identifier(local))
                {
                    return Err(invalid());
                }
                previous.clone_from(&key);
                startup.items.push(content::Item {
                    key,
                    name: display,
                    texture,
                    swatch: [1.0; 4],
                    placeable: None,
                    sprite: true,
                    components: content::Components::None,
                });
            }
            startup.packages.push(composition::Package {
                key: format!("{name}:package"),
                version: 1,
                dependencies: package
                    .dependencies
                    .keys()
                    .map(|name| composition::Dependency {
                        package: format!("{name}:package"),
                        version: 1,
                    })
                    .collect(),
                requires,
            });
        }
        Ok(Some(startup))
    }
}
