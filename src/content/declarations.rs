//! Bounded startup collection, separate from lifecycle ownership and dispatch.
use super::Catalog;
pub(crate) mod budget;
use bloxgloom_host_api::{RegistrationError as Error, composition::Package, content::*};

#[derive(Default)]
pub(crate) struct Declarations {
    textures: Vec<Texture>,
    blocks: Vec<Block>,
    items: Vec<Item>,
    tags: Vec<Tag>,
    packages: Vec<Package>,
    budget: budget::Budget,
}
impl Declarations {
    pub(crate) fn len(&self) -> usize {
        self.textures.len()
            + self.blocks.len()
            + self.items.len()
            + self.tags.len()
            + self.packages.len()
    }

    fn reserve(&mut self, bytes: usize, key: &str) -> Result<(), Error> {
        self.budget.reserve(1, bytes, key)
    }
    pub(crate) fn texture(&mut self, t: Texture) -> Result<(), Error> {
        if t.png.len() > 4 * 1024 * 1024 || t.key.len() > 255 {
            return Err(Error("texture declaration too large".into()));
        }
        self.reserve(t.png.len() + 256, &t.key)?;
        self.textures.push(t);
        Ok(())
    }
    pub(crate) fn block(&mut self, b: Block) -> Result<(), Error> {
        if b.properties.len() > 8
            || b.states.len() > 4096
            || b.key.len() > 255
            || b.name.len() > 255
            || !faces_bounded(&b.textures)
            || b.properties.iter().any(|p| {
                p.name.len() > 255
                    || p.values.len() > 4096
                    || p.values.iter().any(|v| v.len() > 255)
            })
            || b.states.iter().any(|s| {
                s.properties.len() > 8
                    || s.properties
                        .iter()
                        .any(|(k, v)| k.len() > 255 || v.len() > 255)
                    || s.textures.as_ref().is_some_and(|t| !faces_bounded(t))
            })
        {
            return Err(Error("block declaration too large".into()));
        }
        self.reserve(budget::block_bytes(&b), &b.key)?;
        self.blocks.push(b);
        Ok(())
    }
    pub(crate) fn item(&mut self, i: Item) -> Result<(), Error> {
        if i.key.len() > 255
            || i.name.len() > 255
            || i.texture.len() > 255
            || i.placeable.as_ref().is_some_and(|s| s.len() > 512)
        {
            return Err(Error("item declaration too large".into()));
        }
        self.reserve(2048, &i.key)?;
        self.items.push(i);
        Ok(())
    }
    pub(crate) fn tag(&mut self, t: Tag) -> Result<(), Error> {
        if t.key.len() > 255
            || t.members.len() > 4096
            || t.members.iter().any(|m| match m {
                TagMember::Definition(k) | TagMember::Tag(k) => k.len() > 255,
            })
        {
            return Err(Error("tag declaration too large".into()));
        }
        self.reserve(budget::tag_bytes(&t), &t.key)?;
        self.tags.push(t);
        Ok(())
    }
    pub(crate) fn package(&mut self, p: Package) -> Result<(), Error> {
        if p.key.len() > 255
            || p.dependencies.len() > 256
            || p.requires.len() > 32
            || p.dependencies.iter().any(|d| d.package.len() > 255)
            || p.requires.iter().any(|c| c.len() > 255)
        {
            return Err(Error("package declaration too large".into()));
        }
        self.reserve(budget::package_bytes(&p), &p.key)?;
        self.packages.push(p);
        Ok(())
    }
    pub(crate) fn install_base(&mut self, catalog: &mut Catalog) -> Result<(), Error> {
        self.packages.sort_by(|a, b| a.key.cmp(&b.key));
        catalog.composition.packages(&self.packages)?;
        self.textures.sort_by(|a, b| a.key.cmp(&b.key));
        self.blocks.sort_by(|a, b| a.key.cmp(&b.key));
        for t in &self.textures {
            catalog.public_texture(t)?;
        }
        for b in &self.blocks {
            catalog.public_block(b)?;
        }
        Ok(())
    }
    pub(crate) fn install_items_and_tags(&mut self, catalog: &mut Catalog) -> Result<(), Error> {
        self.items.sort_by(|a, b| a.key.cmp(&b.key));
        self.tags
            .sort_by(|a, b| (a.kind, &a.key).cmp(&(b.kind, &b.key)));
        for i in &self.items {
            catalog.public_item(i)?;
        }
        catalog.register_tags(&self.tags)
    }
}
fn faces_bounded(t: &FaceTextures) -> bool {
    [&t.top, &t.side, &t.bottom].iter().all(|s| s.len() <= 255)
}
