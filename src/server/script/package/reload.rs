//! Immutable callback revisions. Only the development coordinator publishes.
use super::*;
use std::collections::BTreeSet;

impl PackageSnapshot {
    pub(in crate::server) fn current(self: &Arc<Self>) -> Arc<Self> {
        self.live
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| Arc::clone(self))
    }

    pub(in crate::server) fn publish(&self, replacement: Arc<Self>) {
        *self.live.write().unwrap_or_else(|e| e.into_inner()) = Some(replacement);
    }

    pub(in crate::server::script) fn mark_generation_module(&self, module: &str) {
        let mut protected = self
            .generation_packages
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut pending = vec![module.split(':').next().unwrap().to_owned()];
        while let Some(package) = pending.pop() {
            if protected.insert(package.clone()) {
                pending.extend(
                    self.packages[&package]
                        .manifest
                        .dependencies
                        .keys()
                        .cloned(),
                );
            }
        }
    }

    /// Pin saved contracts to startup for this process. Structural declarations
    /// still undergo full startup validation and comparison before publication.
    pub(in crate::server) fn replacement(
        self: &Arc<Self>,
        root: &Path,
    ) -> Result<Arc<Self>, ScriptError> {
        let deadline =
            std::time::Instant::now() + crate::server::script::capacity::INSTALLATION_WALL_TIME;
        let mut candidate = Self::discover(root)?;
        if candidate.packages.keys().ne(self.packages.keys()) {
            return Err(error("reload", "restart required: package set changed"));
        }
        let protected: BTreeSet<_> = self
            .generation_packages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for (name, original) in &self.packages {
            let new = &candidate.packages[name];
            if original.manifest != new.manifest {
                return Err(error(name, "restart required: package manifest changed"));
            }
            if protected.contains(name) && original.sources != new.sources {
                return Err(error(
                    name,
                    "restart required: generation package or dependency changed",
                ));
            }
            for (module, source) in &new.sources {
                if std::time::Instant::now() >= deadline {
                    return Err(error(
                        "reload",
                        "replacement source preparation deadline exceeded (10s)",
                    ));
                }
                mlua::chunk::Compiler::new()
                    .compile(format!("return {source}"))
                    .or_else(|_| mlua::chunk::Compiler::new().compile(source))
                    .map_err(|e| error(&format!("{name}:{module}"), e.to_string()))?;
            }
        }
        candidate.compatibility = Some(Arc::downgrade(self));
        Ok(Arc::new(candidate))
    }
}
