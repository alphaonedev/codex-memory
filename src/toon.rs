// Copyright (c) 2026 AlphaOne LLC
// SPDX-License-Identifier: MIT
//
// TOON encoding helpers for compact, token-efficient prompt export.

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;

use crate::model::MemoryRecord;

pub fn encode_memories(memories: &[MemoryRecord]) -> String {
    let header = format!(
        "memories[{}]{{id,kind,scope,source,priority,session,role,tags,created_at,expires_at,summary,content}}:",
        memories.len()
    );

    let rows = memories.iter().map(|memory| {
        [
            escape(memory.id.to_string()),
            escape(memory.kind.to_string()),
            escape(memory.scope.clone()),
            escape(memory.source.clone()),
            escape(memory.priority.to_string()),
            escape(memory.session.clone().unwrap_or_default()),
            escape(memory.role.clone().unwrap_or_default()),
            escape(memory.tags.join("|")),
            escape(memory.created_at.to_rfc3339()),
            escape(
                memory
                    .expires_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_default(),
            ),
            escape(memory.summary.clone().unwrap_or_default()),
            escape(memory.content.clone()),
        ]
        .join("\t")
    });

    std::iter::once(header)
        .chain(rows.map(|row| format!("  {row}")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn encode<T: Serialize>(value: &T, name: &str) -> Result<String> {
    let value = serde_json::to_value(value)?;
    encode_value(&value, name)
}

fn encode_value(value: &Value, name: &str) -> Result<String> {
    match value {
        Value::Array(items) => encode_array(items, name),
        Value::Object(map) => {
            let mut lines = vec![format!("{name}:")];
            for (key, value) in map {
                let encoded = encode_value(value, key)?;
                for line in encoded.lines() {
                    lines.push(format!("  {line}"));
                }
            }
            Ok(lines.join("\n"))
        }
        Value::Null => Ok(format!("{name}: null")),
        Value::Bool(flag) => Ok(format!("{name}: {flag}")),
        Value::Number(number) => Ok(format!("{name}: {number}")),
        Value::String(text) => Ok(format!("{name}: {}", escape(text.clone()))),
    }
}

fn encode_array(items: &[Value], name: &str) -> Result<String> {
    if items.is_empty() {
        return Ok(format!("{name}[0]:"));
    }

    let Some(first) = items.first().and_then(Value::as_object) else {
        let lines = items
            .iter()
            .map(|item| scalar(item).map(|value| format!("  {value}")))
            .collect::<Result<Vec<_>>>()?;
        let mut out = vec![format!("{name}[{}]:", items.len())];
        out.extend(lines);
        return Ok(out.join("\n"));
    };

    let fields = first.keys().cloned().collect::<Vec<_>>();
    let mut out = vec![format!("{name}[{}]{{{}}}:", items.len(), fields.join(","))];
    for item in items {
        let Some(map) = item.as_object() else {
            bail!("mixed array shapes are not supported in TOON encoding");
        };
        let row = fields
            .iter()
            .map(|field| scalar(map.get(field).unwrap_or(&Value::Null)))
            .collect::<Result<Vec<_>>>()?
            .join("\t");
        out.push(format!("  {row}"));
    }
    Ok(out.join("\n"))
}

fn scalar(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Bool(flag) => Ok(flag.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) => Ok(escape(text.clone())),
        other => Ok(escape(serde_json::to_string(other)?)),
    }
}

fn escape(value: String) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::encode;
    use serde_json::json;

    #[test]
    fn encodes_uniform_arrays() {
        let input = json!([
            {"id": 1, "name": "alpha"},
            {"id": 2, "name": "beta"}
        ]);

        let encoded = encode(&input, "items").expect("encode");
        assert!(encoded.starts_with("items[2]{id,name}:"));
        assert!(encoded.contains("1\talpha"));
    }

    #[test]
    fn escapes_special_characters_in_scalar_values() {
        let input = json!({
            "summary": "line1\nline2\tvalue\\tail"
        });

        let encoded = encode(&input, "memory").expect("encode");
        assert!(encoded.contains("line1\\nline2\\tvalue\\\\tail"));
    }
}
