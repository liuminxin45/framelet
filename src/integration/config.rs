//! Edit only one server entry; retain unrelated JSON values and TOML formatting.
use super::Client;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::Path;
pub enum Document {
    Toml(toml_edit::DocumentMut),
    Json(Value),
}
pub struct Config {
    pub before: Option<Vec<u8>>,
    document: Document,
}
impl Config {
    pub fn load(path: &Path, client: Client) -> Result<Self> {
        let before = super::read_optional(path, 8 * 1024 * 1024)?;
        let text =
            std::str::from_utf8(before.as_deref().unwrap_or(b""))?.trim_start_matches('\u{feff}');
        let document = if client == Client::Codex {
            Document::Toml(text.parse::<toml_edit::DocumentMut>().map_err(|_| {
                anyhow::anyhow!("invalid TOML client config; original file preserved")
            })?)
        } else {
            let value = if text.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(text)
                    .context("client config must be valid JSON; comments/JSONC are not rewritten")?
            };
            ensure!(value.is_object(), "client config must be an object");
            Document::Json(value)
        };
        let out = Self { before, document };
        out.entry()?;
        Ok(out)
    }
    pub fn entry(&self) -> Result<Option<Value>> {
        match &self.document {
            Document::Toml(doc) => {
                let Some(servers) = doc.get("mcp_servers") else {
                    return Ok(None);
                };
                ensure!(servers.is_table_like(), "mcp_servers must be a table");
                let Some(entry) = servers.get("framelet") else {
                    return Ok(None);
                };
                ensure!(entry.is_table_like(), "framelet must be a table");
                let mut one = toml_edit::DocumentMut::new();
                one["entry"] = entry.clone();
                let value: Value = toml_edit::de::from_str(&one.to_string()).map_err(|_| {
                    anyhow::anyhow!("invalid Framelet TOML entry; original file preserved")
                })?;
                Ok(Some(value["entry"].clone()))
            }
            Document::Json(doc) => {
                let Some(servers) = doc.get("mcpServers") else {
                    return Ok(None);
                };
                ensure!(servers.is_object(), "mcpServers must be an object");
                let entry = servers.get("framelet").cloned();
                ensure!(
                    entry.as_ref().is_none_or(Value::is_object),
                    "framelet must be an object"
                );
                Ok(entry)
            }
        }
    }
    pub fn render(mut self, entry: Option<&Value>) -> Result<Vec<u8>> {
        Ok(match &mut self.document {
            Document::Toml(doc) => {
                if let Some(entry) = entry {
                    let mut replacement =
                        toml_edit::ser::to_document(&serde_json::json!({"framelet":entry}))?;
                    if doc.get("mcp_servers").is_none() {
                        let mut t = toml_edit::Table::new();
                        t.set_implicit(true);
                        doc["mcp_servers"] = toml_edit::Item::Table(t);
                    }
                    let table = replacement
                        .as_table_mut()
                        .remove("framelet")
                        .context("entry serialization missing")?
                        .into_table()
                        .map_err(|_| anyhow::anyhow!("entry table missing"))?;
                    doc["mcp_servers"]["framelet"] = toml_edit::Item::Value(
                        toml_edit::Value::InlineTable(table.into_inline_table()),
                    );
                } else if let Some(servers) = doc
                    .get_mut("mcp_servers")
                    .and_then(toml_edit::Item::as_table_like_mut)
                {
                    servers.remove("framelet");
                }
                doc.to_string().into_bytes()
            }
            Document::Json(doc) => {
                if let Some(entry) = entry {
                    if doc.get("mcpServers").is_none() {
                        doc["mcpServers"] = serde_json::json!({});
                    }
                    doc["mcpServers"]["framelet"] = entry.clone();
                } else if let Some(servers) =
                    doc.get_mut("mcpServers").and_then(Value::as_object_mut)
                {
                    servers.remove("framelet");
                }
                let mut bytes = serde_json::to_vec_pretty(doc)?;
                bytes.push(b'\n');
                bytes
            }
        })
    }
}
