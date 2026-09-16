use serde::{Deserialize, Serialize};
use toml_edit::{table, value, DocumentMut, Item, Table};

use crate::extensions::mcp::observe::CODEX_MODELED_FIELDS;
use crate::extensions::mcp::render::CodexServerRender;

/// One document entry change: canonical before/after text of exactly the
/// owned entry. `before`/`after` are `None` when the entry did not exist or
/// was removed. Values are raw; previews redact them at view time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryChange {
    pub pointer: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Applies server patches to a Codex document. Each patch replaces exactly
/// one `mcp_servers.<key>` table (or removes it); every other byte of the
/// document is preserved. `rendered_entry` returns the canonical text of the
/// written entry for baseline bookkeeping.
pub fn apply_codex_server_patches(
    document: &str,
    patches: &[(String, Option<CodexServerRender>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut doc = crate::adapter::codex::parse(document)?;
    let mut changes = Vec::new();
    if !doc.contains_key("mcp_servers") && patches.iter().any(|(_, patch)| patch.is_some()) {
        doc["mcp_servers"] = table();
    }
    let Some(Item::Table(mcp_table)) = doc.get_mut("mcp_servers") else {
        // No patches to write and no table; nothing to do.
        return Ok((doc.to_string(), changes));
    };
    for (key, patch) in patches {
        let before = mcp_table
            .get(key)
            .map(|item| render_codex_entry_text(key, item))
            .filter(|text| !text.trim().is_empty());
        match patch {
            Some(render) => {
                // The application owns the modeled fields; unknown host
                // fields inside the same server entry are carried over
                // verbatim so an edit never deletes them.
                let mut table = codex_render_table(render);
                if let Some(existing) = mcp_table.get(key).and_then(|item| item.as_table()) {
                    for (name, item) in existing.iter() {
                        if !CODEX_MODELED_FIELDS.contains(&name) && table.get(name).is_none() {
                            table.insert(name, item.clone());
                        }
                    }
                }
                let after = render_codex_entry_text(key, &Item::Table(table.clone()));
                mcp_table.insert(key, Item::Table(table));
                changes.push(EntryChange {
                    pointer: format!("mcp_servers.{key}"),
                    before,
                    after: Some(after),
                });
            }
            None => {
                if before.is_some() {
                    mcp_table.remove(key);
                    changes.push(EntryChange {
                        pointer: format!("mcp_servers.{key}"),
                        before,
                        after: None,
                    });
                }
            }
        }
    }
    if let Item::Table(mcp_table) = &doc["mcp_servers"] {
        if mcp_table.is_empty() {
            doc.remove("mcp_servers");
        }
    }
    Ok((doc.to_string(), changes))
}

/// Renders one `mcp_servers.<key>` entry (including nested tables) by
/// placing it into a scratch document, so the canonical text matches what
/// the real document looks like. Also the canonical form of
/// `EntryChange.before/after`, and therefore the comparison basis for the
/// entry-level ownership checks (`managed_entry_text`).
pub(crate) fn render_codex_entry_text(key: &str, item: &Item) -> String {
    let mut scratch = DocumentMut::new();
    scratch["mcp_servers"] = table();
    if let Some(scratch_table) = scratch
        .get_mut("mcp_servers")
        .and_then(|entry| entry.as_table_like_mut())
    {
        scratch_table.insert(key, item.clone());
    }
    scratch.to_string()
}

fn codex_render_table(render: &CodexServerRender) -> Table {
    let mut table = Table::new();
    match render {
        CodexServerRender::Stdio {
            command,
            args,
            env,
            env_vars,
            options,
            enabled,
        } => {
            table.insert("command", value(command.as_str()));
            if !args.is_empty() {
                let mut array = toml_edit::Array::new();
                for argument in args {
                    array.push(argument.as_str());
                }
                table.insert("args", value(array));
            }
            if !env.is_empty() {
                let mut env_table = Table::new();
                for (name, val) in env {
                    env_table.insert(name, value(val.as_str()));
                }
                table.insert("env", Item::Table(env_table));
            }
            if !env_vars.is_empty() {
                let mut array = toml_edit::Array::new();
                for name in env_vars {
                    array.push(name.as_str());
                }
                table.insert("env_vars", value(array));
            }
            if let Some(options) = options {
                if let Some(cwd) = &options.cwd {
                    table.insert("cwd", value(cwd.as_str()));
                }
                if let Some(timeout) = options.startup_timeout_sec {
                    table.insert(
                        "startup_timeout_sec",
                        value(i64::try_from(timeout).unwrap_or(i64::MAX)),
                    );
                }
                if let Some(timeout) = options.tool_timeout_sec {
                    table.insert(
                        "tool_timeout_sec",
                        value(i64::try_from(timeout).unwrap_or(i64::MAX)),
                    );
                }
                if let Some(required) = options.required {
                    table.insert("required", value(required));
                }
            }
            table.insert("enabled", value(*enabled));
        }
        CodexServerRender::Http {
            url,
            bearer_token_env_var,
            http_headers,
            env_http_headers,
            enabled,
        } => {
            table.insert("url", value(url.as_str()));
            if let Some(name) = bearer_token_env_var {
                table.insert("bearer_token_env_var", value(name.as_str()));
            }
            if !http_headers.is_empty() {
                let mut headers = Table::new();
                for (name, val) in http_headers {
                    headers.insert(name, value(val.as_str()));
                }
                table.insert("http_headers", Item::Table(headers));
            }
            if !env_http_headers.is_empty() {
                let mut headers = Table::new();
                for (name, variable) in env_http_headers {
                    headers.insert(name, value(variable.as_str()));
                }
                table.insert("env_http_headers", Item::Table(headers));
            }
            table.insert("enabled", value(*enabled));
        }
    }
    table
}

/// Restores Codex entry text captured in a baseline. `original` is the exact
/// canonical render of the previous `mcp_servers.<key>` entry — a whole
/// document fragment rooted at `mcp_servers` — or `None` when the entry did
/// not exist.
pub fn apply_codex_entry_restore(
    document: &str,
    key: &str,
    original: Option<&str>,
) -> Result<String, crate::adapter::AdapterError> {
    let mut doc = crate::adapter::codex::parse(document)?;
    let Some(original) = original else {
        if let Some(Item::Table(mcp_table)) = doc.get_mut("mcp_servers") {
            mcp_table.remove(key);
            if mcp_table.is_empty() {
                doc.remove("mcp_servers");
            }
        }
        return Ok(doc.to_string());
    };
    let mut parsed: DocumentMut =
        original
            .parse()
            .map_err(|error| crate::adapter::AdapterError {
                message: format!("备份的原始条目无法解析：{error}"),
                line: None,
            })?;
    let restored = parsed
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_mut())
        .and_then(|table| table.remove(key))
        .ok_or_else(|| crate::adapter::AdapterError {
            message: format!("备份的原始条目缺少 mcp_servers.{key}"),
            line: None,
        })?;
    if !doc.contains_key("mcp_servers") {
        doc["mcp_servers"] = table();
    }
    if let Some(mcp_servers) = doc
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_like_mut())
    {
        mcp_servers.insert(key, restored);
    }
    Ok(doc.to_string())
}
