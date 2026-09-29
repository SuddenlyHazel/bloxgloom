//! Typed, bounded presentation values shared by materials and effect passes.
use serde::Deserialize;

#[cfg(test)]
mod tests;

pub(crate) const MAX_PARAMETERS: usize = 8;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Float,
    Uint,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    Color,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum Value {
    Bool(bool),
    Scalar(f32),
    Vector(Vec<f32>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Definition {
    pub name: String,
    pub kind: Kind,
    pub default: Value,
    pub min: Option<f32>,
    pub max: Option<f32>,
}

impl Definition {
    pub(crate) fn pack(&self, value: &Value) -> Result<[f32; 4], String> {
        let mut result = [0.0; 4];
        match (self.kind, value) {
            (Kind::Bool, Value::Bool(value)) => result[0] = u8::from(*value) as f32,
            (Kind::Float | Kind::Uint, Value::Scalar(value)) => {
                if self.kind == Kind::Uint
                    && (*value < 0.0 || *value > 16_777_215.0 || value.fract() != 0.0)
                {
                    return Err(format!("{}: expected an exact unsigned integer", self.name));
                }
                result[0] = *value;
            }
            (kind, Value::Vector(values)) => {
                let count = match kind {
                    Kind::Vec2 => 2,
                    Kind::Vec3 => 3,
                    Kind::Vec4 | Kind::Color => 4,
                    _ => 0,
                };
                if count == 0 || values.len() != count {
                    return Err(format!("{}: parameter type mismatch", self.name));
                }
                result[..count].copy_from_slice(values);
            }
            _ => return Err(format!("{}: parameter type mismatch", self.name)),
        }
        let min = self.min.unwrap_or(if self.kind == Kind::Color {
            0.0
        } else {
            -1_000_000.0
        });
        let max = self.max.unwrap_or(if self.kind == Kind::Color {
            1.0
        } else {
            1_000_000.0
        });
        let count = match self.kind {
            Kind::Vec2 => 2,
            Kind::Vec3 => 3,
            Kind::Vec4 | Kind::Color => 4,
            _ => 1,
        };
        if !min.is_finite()
            || !max.is_finite()
            || min > max
            || result[..count]
                .iter()
                .any(|v| !v.is_finite() || *v < min || *v > max)
        {
            return Err(format!("{}: parameter outside its finite range", self.name));
        }
        Ok(result)
    }
}

pub(crate) fn defaults(definitions: &[Definition]) -> Result<[[f32; 4]; MAX_PARAMETERS], String> {
    if definitions.len() > MAX_PARAMETERS {
        return Err("at most eight parameters per visual resource".into());
    }
    let mut result = [[0.0; 4]; MAX_PARAMETERS];
    let mut names = std::collections::BTreeSet::new();
    for (index, definition) in definitions.iter().enumerate() {
        if !identifier(&definition.name) || !names.insert(&definition.name) {
            return Err("invalid or duplicate parameter name".into());
        }
        result[index] = definition.pack(&definition.default)?;
    }
    Ok(result)
}

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[derive(Clone, Debug)]
pub(crate) struct Update {
    pub resource: String,
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    definitions: std::collections::BTreeMap<String, Vec<Definition>>,
    pending: std::collections::BTreeMap<(String, String), Value>,
}
impl State {
    pub(crate) fn register(
        &mut self,
        resource: &str,
        definitions: &[Definition],
    ) -> Result<(), String> {
        defaults(definitions)?;
        if self.definitions.len() >= 32 || self.definitions.contains_key(resource) {
            return Err(format!(
                "{resource}: duplicate visual resource or resource limit exceeded"
            ));
        }
        self.definitions
            .insert(resource.into(), definitions.to_vec());
        Ok(())
    }
    pub(crate) fn check(&self, owner: &str, update: &Update) -> Result<(), String> {
        if update
            .resource
            .split_once(':')
            .is_none_or(|(namespace, _)| namespace != owner)
        {
            return Err(format!(
                "{}: foreign visual parameter target",
                update.resource
            ));
        }
        self.definitions
            .get(&update.resource)
            .and_then(|definitions| definitions.iter().find(|d| d.name == update.name))
            .ok_or_else(|| format!("{}: unknown parameter {}", update.resource, update.name))?
            .pack(&update.value)?;
        Ok(())
    }
    pub(crate) fn apply(&mut self, owner: &str, updates: &[Update]) -> Result<(), String> {
        for update in updates {
            self.check(owner, update)?;
        }
        for update in updates {
            self.pending.insert(
                (update.resource.clone(), update.name.clone()),
                update.value.clone(),
            );
        }
        Ok(())
    }
    pub(crate) fn take_updates(&mut self) -> Vec<Update> {
        std::mem::take(&mut self.pending)
            .into_iter()
            .map(|((resource, name), value)| Update {
                resource,
                name,
                value,
            })
            .collect()
    }
}
