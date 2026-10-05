//! Render changed containers while retaining original JSON member/value text.
//! serde parses JSON; this module only splices already validated raw fragments.
use crate::cli::Result;
use serde::de::{Deserialize, Deserializer, Error, MapAccess, Visitor};
use serde_json::{value::RawValue, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Reject duplicate keys before merging: maps alone lose their original order
/// and cannot safely reconstruct raw fragments containing duplicate members.
pub(crate) fn parse(raw: &str) -> Result<Value> {
    // Validate syntax and serde's nesting limit before the raw-value traversal.
    let value = serde_json::from_str(raw)?;
    unique_keys(raw)?;
    Ok(value)
}

fn unique_keys(raw: &str) -> serde_json::Result<()> {
    match raw.trim_start().as_bytes().first() {
        Some(b'{') => {
            serde_json::from_str::<UniqueObject>(raw)?;
        }
        Some(b'[') => {
            for value in serde_json::from_str::<Vec<&RawValue>>(raw)? {
                unique_keys(value.get())?;
            }
        }
        _ => (),
    }
    Ok(())
}

struct UniqueObject;
impl<'de> Deserialize<'de> for UniqueObject {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = UniqueObject;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object with unique keys")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(A::Error::custom(format!("duplicate object key: {key:?}")));
                    }
                    let value = map.next_value::<&RawValue>()?;
                    unique_keys(value.get()).map_err(A::Error::custom)?;
                }
                Ok(UniqueObject)
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

pub(crate) fn render(raw: &str, new: &Value) -> Result<String> {
    let old: Value = serde_json::from_str(raw)?;
    if old == *new {
        return Ok(raw.into());
    }
    let trimmed = raw.trim();
    let body = match (&old, new) {
        (Value::Object(_), Value::Object(new)) => {
            let members: BTreeMap<String, &RawValue> = serde_json::from_str(trimmed)?;
            let mut ordered: Vec<_> = members.iter().collect();
            ordered.sort_by_key(|(_, v)| v.get().as_ptr() as usize);
            let mut fragments = BTreeMap::new();
            let mut end = 1;
            for (key, value) in ordered {
                let start = value.get().as_ptr() as usize - trimmed.as_ptr() as usize;
                let prefix = &trimmed[end..start];
                let prefix = if end == 1 {
                    prefix.to_owned()
                } else {
                    prefix.replacen(',', "", 1)
                };
                fragments.insert(key, (prefix, value.get()));
                end = start + value.get().len();
            }
            let mut entries = Vec::new();
            for (key, value) in new {
                entries.push(if let Some((prefix, raw)) = fragments.get(key) {
                    format!("{prefix}{}", render(raw, value)?)
                } else {
                    format!(
                        "\n  {}: {}",
                        serde_json::to_string(key)?,
                        serde_json::to_string(value)?
                    )
                });
            }
            format!(
                "{{{}{}}}",
                entries.join(","),
                &trimmed[end..trimmed.len() - 1]
            )
        }
        (Value::Array(old), Value::Array(new)) => {
            let values: Vec<&RawValue> = serde_json::from_str(trimmed)?;
            let mut used = vec![false; values.len()];
            let mut entries = Vec::new();
            for (index, value) in new.iter().enumerate() {
                let found = old
                    .iter()
                    .enumerate()
                    .position(|(i, v)| !used[i] && v == value)
                    .or_else(|| (index < old.len() && !used[index]).then_some(index));
                if let Some(i) = found {
                    used[i] = true;
                    let start = if i == 0 {
                        1
                    } else {
                        values[i - 1].get().as_ptr() as usize - trimmed.as_ptr() as usize
                            + values[i - 1].get().len()
                    };
                    let end = values[i].get().as_ptr() as usize - trimmed.as_ptr() as usize;
                    let prefix = &trimmed[start..end];
                    let prefix = if i == 0 {
                        prefix.to_owned()
                    } else {
                        prefix.replacen(',', "", 1)
                    };
                    entries.push(format!("{}{}", prefix, render(values[i].get(), value)?));
                } else {
                    entries.push(serde_json::to_string(value)?);
                }
            }
            let end = values.last().map_or(1, |v| {
                v.get().as_ptr() as usize - trimmed.as_ptr() as usize + v.get().len()
            });
            format!(
                "[{}{}]",
                entries.join(","),
                &trimmed[end..trimmed.len() - 1]
            )
        }
        _ => serde_json::to_string(new)?,
    };
    let start = raw.len() - raw.trim_start().len();
    Ok(format!(
        "{}{}{}",
        &raw[..start],
        body,
        &raw[start + trimmed.len()..]
    ))
}
