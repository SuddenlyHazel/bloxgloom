//! Frozen package contracts and union-composed tags, including manifest identity.
use super::{Catalog, hash_bytes, valid_key};
use bloxgloom_host_api::{
    RegistrationError as Error, composition as api,
    content::{Tag, TagKind, TagMember},
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PACKAGES: usize = 256;
const MAX_TAGS: usize = 1024;
const MAX_MEMBERS: usize = 65_536;

#[derive(Clone, Debug)]
struct Package {
    id: u32,
    definition: api::Package,
    fingerprint: u64,
}
#[derive(Clone, Debug)]
struct TagDefinition {
    id: u32,
    definition: Tag,
    resolved: BTreeSet<String>,
    fingerprint: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Composition {
    packages: BTreeMap<String, Package>,
    tags: BTreeMap<(TagKind, String), TagDefinition>,
}

impl Composition {
    pub(super) fn identities(&self) -> Vec<(u8, u32, &str, u64)> {
        self.packages
            .iter()
            .map(|(key, p)| (b'P', p.id, key.as_str(), p.fingerprint))
            .chain(
                self.tags
                    .iter()
                    .map(|((kind, key), t)| (tag_kind(*kind), t.id, key.as_str(), t.fingerprint)),
            )
            .collect()
    }

    pub(super) fn remap(&mut self, kind: u8, key: &str, id: u32) -> bool {
        if kind == b'P' {
            let Some(p) = self.packages.get_mut(key) else {
                return false;
            };
            p.id = id;
        } else {
            let kind = if kind == b'T' {
                TagKind::Block
            } else if kind == b'U' {
                TagKind::Item
            } else {
                return false;
            };
            let Some(t) = self.tags.get_mut(&(kind, key.to_owned())) else {
                return false;
            };
            t.id = id;
        }
        true
    }

    pub(crate) fn packages(&mut self, definitions: &[api::Package]) -> Result<(), Error> {
        if self.packages.len() + definitions.len() > MAX_PACKAGES {
            return Err(Error("too many packages".into()));
        }
        for definition in definitions {
            if !valid_key(&definition.key)
                || definition.key == "bloxgloom:core"
                || definition.version == 0
                || definition.dependencies.len() > MAX_PACKAGES
                || definition.requires.len() > 32
            {
                return Err(Error(format!("invalid package {}", definition.key)));
            }
            if self.packages.contains_key(&definition.key) {
                return Err(Error(format!("duplicate package {}", definition.key)));
            }
            let mut definition = definition.clone();
            definition
                .dependencies
                .sort_by(|a, b| a.package.cmp(&b.package));
            definition.requires.sort();
            if definition
                .dependencies
                .windows(2)
                .any(|w| w[0].package == w[1].package)
                || definition.requires.windows(2).any(|w| w[0] == w[1])
            {
                return Err(Error(format!(
                    "duplicate package requirement {}",
                    definition.key
                )));
            }
            let mut hash = 0xcbf29ce484222325;
            field(&mut hash, definition.key.as_bytes());
            field(&mut hash, &definition.version.to_le_bytes());
            field(
                &mut hash,
                &(definition.dependencies.len() as u32).to_le_bytes(),
            );
            for d in &definition.dependencies {
                field(&mut hash, d.package.as_bytes());
                field(&mut hash, &d.version.to_le_bytes());
            }
            field(&mut hash, &(definition.requires.len() as u32).to_le_bytes());
            for c in &definition.requires {
                if ![
                    api::CONTENT,
                    api::STORAGE,
                    api::MACHINES,
                    api::MOBILE_ENTITIES,
                    api::INVENTORY_SCREENS,
                    api::ANCHORED_ENTITIES,
                    api::ACTIONS,
                    api::OWNER_SYSTEMS,
                    api::ITEM_ICONS,
                    api::GENERATION,
                ]
                .contains(&c.as_str())
                {
                    return Err(Error(format!(
                        "{} requires unsupported capability {c}",
                        definition.key
                    )));
                }
                field(&mut hash, c.as_bytes());
            }
            let id = self.packages.values().map(|p| p.id + 1).max().unwrap_or(0);
            if id >= super::MAX_ASSIGNED_ID {
                return Err(Error("package identity range exhausted".into()));
            }
            self.packages.insert(
                definition.key.clone(),
                Package {
                    id,
                    definition,
                    fingerprint: hash,
                },
            );
        }
        // Iterative bounded topological validation; no recursive extension-controlled stack.
        let mut ready = BTreeSet::from(["bloxgloom:core"]);
        while ready.len() <= self.packages.len() {
            let before = ready.len();
            for (key, p) in &self.packages {
                for d in &p.definition.dependencies {
                    let version = if d.package == "bloxgloom:core" {
                        Some(1)
                    } else {
                        self.packages.get(&d.package).map(|p| p.definition.version)
                    };
                    if version != Some(d.version) {
                        return Err(Error(format!(
                            "{key}: missing or incompatible dependency {} v{}",
                            d.package, d.version
                        )));
                    }
                }
                if p.definition
                    .dependencies
                    .iter()
                    .all(|d| ready.contains(d.package.as_str()))
                {
                    ready.insert(key);
                }
            }
            if ready.len() == before {
                return Err(Error("package dependency cycle".into()));
            }
        }
        Ok(())
    }

    pub(crate) fn item_tag(&self, key: &str) -> Option<&BTreeSet<String>> {
        self.tags
            .get(&(TagKind::Item, key.to_owned()))
            .map(|t| &t.resolved)
    }
}

impl Catalog {
    pub(crate) fn register_tags(&mut self, declarations: &[Tag]) -> Result<(), Error> {
        let mut tags = self.composition.tags.clone();
        for definition in declarations {
            if !valid_key(&definition.key) || definition.members.len() > 4096 {
                return Err(Error("invalid tag".into()));
            }
            if self
                .composition
                .tags
                .contains_key(&(definition.kind, definition.key.clone()))
            {
                return Err(Error(format!(
                    "tag {} is already installed; submit contributors in one bundle",
                    definition.key
                )));
            }
            let id = tags
                .iter()
                .filter(|((kind, _), _)| *kind == definition.kind)
                .map(|(_, t)| t.id + 1)
                .max()
                .unwrap_or(0);
            if id >= super::MAX_ASSIGNED_ID {
                return Err(Error("tag identity range exhausted".into()));
            }
            let tag = tags
                .entry((definition.kind, definition.key.clone()))
                .or_insert_with(|| TagDefinition {
                    id,
                    definition: Tag {
                        members: vec![],
                        ..definition.clone()
                    },
                    resolved: BTreeSet::new(),
                    fingerprint: 0,
                });
            tag.definition.members.extend(definition.members.clone());
        }
        if tags.len() > MAX_TAGS
            || tags
                .values()
                .map(|t| t.definition.members.len())
                .sum::<usize>()
                > MAX_MEMBERS
        {
            return Err(Error("tag declaration budget exceeded".into()));
        }
        let mut ready = BTreeSet::new();
        let mut total = 0;
        let keys = tags.keys().cloned().collect::<Vec<_>>();
        while ready.len() < tags.len() {
            let before = ready.len();
            for key in keys.iter().cloned() {
                if ready.contains(&key) {
                    continue;
                }
                let t = &tags[&key];
                let mut blocked = false;
                // Check readiness before copying expanded memberships. A deep
                // dependency chain must not repeatedly allocate a large prefix.
                for member in &t.definition.members {
                    if let TagMember::Tag(reference) = member {
                        let reference = (t.definition.kind, reference.clone());
                        if !tags.contains_key(&reference) {
                            return Err(Error(format!("missing nested tag {}", reference.1)));
                        }
                        blocked |= !ready.contains(&reference);
                    }
                }
                if blocked {
                    continue;
                }
                let mut resolved = BTreeSet::new();
                for member in &t.definition.members {
                    match member {
                        TagMember::Definition(key) => {
                            let valid = match t.definition.kind {
                                TagKind::Block => self.block_keys.contains(key),
                                TagKind::Item => self.item_keys.contains(key),
                            };
                            if !valid {
                                return Err(Error(format!(
                                    "{}: missing tag member {key}",
                                    t.definition.key
                                )));
                            }
                            resolved.insert(key.clone());
                        }
                        TagMember::Tag(reference) => {
                            let reference = (t.definition.kind, reference.clone());
                            let other = &tags[&reference];
                            for member in &other.resolved {
                                resolved.insert(member.clone());
                                if total + resolved.len() > MAX_MEMBERS {
                                    return Err(Error("resolved tag budget exceeded".into()));
                                }
                            }
                        }
                    }
                    if total + resolved.len() > MAX_MEMBERS {
                        return Err(Error("resolved tag budget exceeded".into()));
                    }
                }
                total += resolved.len();
                let t = tags.get_mut(&key).unwrap();
                t.resolved = resolved;
                let mut hash = 0xcbf29ce484222325;
                field(&mut hash, &[tag_kind(key.0)]);
                field(&mut hash, key.1.as_bytes());
                for key in &t.resolved {
                    field(&mut hash, key.as_bytes());
                }
                t.fingerprint = hash;
                ready.insert(key);
            }
            if ready.len() == before {
                return Err(Error("tag dependency cycle".into()));
            }
        }
        self.composition.tags = tags;
        Ok(())
    }

    /// Compiled machine allowlists accept exact keys or `#namespace:item_tag`.
    pub(crate) fn expand_item_filter(&self, references: &[String]) -> Result<Vec<String>, Error> {
        let mut keys = BTreeSet::new();
        for reference in references {
            if let Some(tag) = reference.strip_prefix('#') {
                let members = self
                    .composition
                    .item_tag(tag)
                    .ok_or_else(|| Error(format!("missing item tag {tag}")))?;
                // An empty explicit allowlist means any item, so reject empty tags here.
                if members.is_empty() {
                    return Err(Error(format!("empty machine item tag {tag}")));
                }
                for member in members {
                    keys.insert(member.clone());
                    if keys.len() > 4096 {
                        return Err(Error("machine filter expansion exceeds 4096".into()));
                    }
                }
            } else {
                keys.insert(reference.clone());
            }
            if keys.len() > 4096 {
                return Err(Error("machine filter expansion exceeds 4096".into()));
            }
        }
        Ok(keys.into_iter().collect())
    }
}

fn tag_kind(kind: TagKind) -> u8 {
    match kind {
        TagKind::Block => b'T',
        TagKind::Item => b'U',
    }
}
fn field(hash: &mut u64, bytes: &[u8]) {
    hash_bytes(hash, &(bytes.len() as u64).to_le_bytes());
    hash_bytes(hash, bytes);
}
