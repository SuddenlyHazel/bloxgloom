//! Bounded startup tag contributions. The shared catalog resolves references,
//! same-kind nesting and cycles only after every package has declared content.
use super::*;
use bloxgloom_host_api::content::{Tag, TagKind, TagMember};

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    permitted: bool,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, (key, kind, members): (Value, Value, Value)| {
        let mut pending = pending.borrow_mut();
        let result = (|| {
            if let Some(error) = pending.error {
                return Err(error);
            }
            if !permitted {
                return Err("register_tag requires bloxgloom:content/v1");
            }
            if pending.tags.len() >= 32 {
                return Err("startup tag limit exceeded (32 per package)");
            }
            let key = text(key)?;
            if key.split_once(':').is_none_or(|(owner, local)| {
                owner != namespace || !super::super::package::manifest::identifier(local)
            }) {
                return Err("tag key must belong to the startup package");
            }
            let kind = match text(kind)?.as_str() {
                "block" => TagKind::Block,
                "item" => TagKind::Item,
                _ => return Err("tag kind must be block or item"),
            };
            let Value::Table(table) = members else {
                return Err("tag members must be an array");
            };
            if table.metatable().is_some() || table.raw_len() == 0 || table.raw_len() > 32 {
                return Err("tag must have 1..32 members without a metatable");
            }
            let mut parsed = Vec::with_capacity(table.raw_len());
            for (index, pair) in table.clone().pairs::<Value, Value>().enumerate() {
                if index >= 33 {
                    return Err("tag member limit exceeded");
                }
                let (index_key, value) = pair.map_err(|_| "invalid tag member")?;
                if !matches!(index_key, Value::Integer(n) if (1..=32).contains(&n)) {
                    return Err("tag members must use array indices");
                }
                let member = text(value)?;
                let (nested, key) = if let Some(key) = member.strip_prefix('#') {
                    (true, key)
                } else {
                    (false, member.as_str())
                };
                if key.split_once(':').is_none_or(|(owner, local)| {
                    !super::super::package::manifest::identifier(owner)
                        || !super::super::package::manifest::identifier(local)
                }) {
                    return Err("invalid tag member key");
                }
                parsed.push(if nested {
                    TagMember::Tag(key.to_owned())
                } else {
                    TagMember::Definition(key.to_owned())
                });
            }
            if parsed.len() != table.raw_len() {
                return Err("tag members must form a dense array");
            }
            parsed.sort_by(|a, b| member_key(a).cmp(&member_key(b)));
            if parsed
                .windows(2)
                .any(|pair| member_key(&pair[0]) == member_key(&pair[1]))
            {
                return Err("duplicate tag member");
            }
            pending.tags.push(Tag {
                key,
                kind,
                members: parsed,
            });
            Ok(())
        })();
        result.map_err(|error| {
            pending.error.get_or_insert(error);
            mlua::Error::RuntimeError(error.into())
        })
    })
}

pub(in crate::server::script) fn member_key(member: &TagMember) -> (u8, &str) {
    match member {
        TagMember::Definition(key) => (0, key),
        TagMember::Tag(key) => (1, key),
    }
}
