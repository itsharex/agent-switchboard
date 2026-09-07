use std::collections::BTreeMap;

use crate::extensions::contracts::{FieldEdit, McpDefinition, McpEditRequest, SecretValue};
use crate::extensions::validate::{
    validate_mcp_definition, validate_server_key, ExtensionValidationError,
};

fn reject<T>(
    field: &'static str,
    message: impl Into<String>,
) -> Result<T, ExtensionValidationError> {
    Err(ExtensionValidationError {
        field,
        message: message.into(),
    })
}

/// A required scalar position: keep (absent) or replace; delete is refused.
fn apply_required<T: Clone>(
    field: &'static str,
    edit: Option<&FieldEdit<T>>,
    current: T,
) -> Result<T, ExtensionValidationError> {
    match edit {
        None => Ok(current),
        Some(FieldEdit::Replace { value }) => Ok(value.clone()),
        Some(FieldEdit::Delete) => reject(
            field,
            format!("{}是必填字段，只能保留或替换，不能删除", field_label(field)),
        ),
    }
}

fn field_label(field: &str) -> &'static str {
    match field {
        "command" => "启动命令",
        "url" => "服务地址",
        _ => "该字段",
    }
}

/// Applies one optional-position edit: absent keeps, replace overwrites,
/// delete removes.
fn apply_optional<T: Clone>(
    current: Option<T>,
    edit: Option<&FieldEdit<T>>,
) -> Result<Option<T>, ExtensionValidationError> {
    match edit {
        None => Ok(current),
        Some(FieldEdit::Replace { value }) => Ok(Some(value.clone())),
        Some(FieldEdit::Delete) => Ok(None),
    }
}

/// Applies per-slot edits to one named secret map. Slots absent from the
/// edit map are copied from the current definition unchanged.
fn apply_secret_slots(
    current: BTreeMap<String, SecretValue>,
    edits: &BTreeMap<String, FieldEdit<SecretValue>>,
) -> Result<BTreeMap<String, SecretValue>, ExtensionValidationError> {
    let mut next = current;
    for (name, edit) in edits {
        match edit {
            FieldEdit::Replace { value } => {
                next.insert(name.clone(), value.clone());
            }
            FieldEdit::Delete => {
                next.remove(name);
            }
        }
    }
    Ok(next)
}

/// Merges one edit request into the current definition and returns the next
/// `(server key, definition)` pair. The result is validated as a whole by
/// the caller through the ordinary definition validator.
pub fn apply_mcp_edit(
    current_name: &str,
    current: &McpDefinition,
    request: &McpEditRequest,
) -> Result<(String, McpDefinition), ExtensionValidationError> {
    if let Some(server_key) = &request.server_key {
        if server_key != current_name {
            validate_server_key(server_key)?;
        }
    }
    let name = request
        .server_key
        .clone()
        .unwrap_or_else(|| current_name.to_string());

    let definition = match &request.transport {
        // A transport switch replaces the whole variant: no field of the old
        // transport can be carried over, so the replacement must already be
        // complete and field-level edits alongside it are contradictory.
        Some(replacement) => {
            if !request.fields.is_empty() {
                return reject(
                    "transport",
                    "切换传输时必须完整提供新传输配置，不能同时提交字段级修改",
                );
            }
            replacement.clone()
        }
        None => edit_kept_transport(current, &request.fields)?,
    };
    validate_mcp_definition(&definition)?;
    Ok((name, definition))
}

fn edit_kept_transport(
    current: &McpDefinition,
    fields: &crate::extensions::contracts::McpFieldEdits,
) -> Result<McpDefinition, ExtensionValidationError> {
    use crate::extensions::contracts::McpFieldEdits;

    let McpFieldEdits {
        command,
        args,
        env,
        url,
        headers,
        bearer,
        codex_options,
    } = fields;
    match current {
        McpDefinition::Stdio {
            command: current_command,
            args: current_args,
            env: current_env,
            codex_options: current_options,
        } => {
            reject_inapplicable("url", url.as_ref(), "HTTP")?;
            reject_inapplicable_map("headers", headers, "HTTP/SSE/WebSocket")?;
            reject_inapplicable("bearer", bearer.as_ref(), "HTTP")?;
            Ok(McpDefinition::Stdio {
                command: apply_required("command", command.as_ref(), current_command.clone())?,
                args: match args.as_ref() {
                    Some(FieldEdit::Replace { value }) => value.clone(),
                    Some(FieldEdit::Delete) => Vec::new(),
                    None => current_args.clone(),
                },
                env: apply_secret_slots(current_env.clone(), env)?,
                codex_options: apply_optional(current_options.clone(), codex_options.as_ref())?,
            })
        }
        McpDefinition::Http {
            url: current_url,
            headers: current_headers,
            bearer: current_bearer,
        } => {
            reject_inapplicable("command", command.as_ref(), "stdio")?;
            reject_inapplicable("args", args.as_ref(), "stdio")?;
            reject_inapplicable_map("env", env, "stdio")?;
            reject_inapplicable("codexOptions", codex_options.as_ref(), "stdio")?;
            Ok(McpDefinition::Http {
                url: apply_required("url", url.as_ref(), current_url.clone())?,
                headers: apply_secret_slots(current_headers.clone(), headers)?,
                bearer: apply_optional(current_bearer.clone(), bearer.as_ref())?,
            })
        }
        McpDefinition::ClaudeSse {
            url: current_url,
            headers: current_headers,
        }
        | McpDefinition::ClaudeWs {
            url: current_url,
            headers: current_headers,
        } => {
            reject_inapplicable("command", command.as_ref(), "stdio")?;
            reject_inapplicable("args", args.as_ref(), "stdio")?;
            reject_inapplicable_map("env", env, "stdio")?;
            reject_inapplicable("bearer", bearer.as_ref(), "HTTP")?;
            reject_inapplicable("codexOptions", codex_options.as_ref(), "stdio")?;
            let next_url = apply_required("url", url.as_ref(), current_url.clone())?;
            let next_headers = apply_secret_slots(current_headers.clone(), headers)?;
            Ok(match current {
                McpDefinition::ClaudeSse { .. } => McpDefinition::ClaudeSse {
                    url: next_url,
                    headers: next_headers,
                },
                McpDefinition::ClaudeWs { .. } => McpDefinition::ClaudeWs {
                    url: next_url,
                    headers: next_headers,
                },
                _ => unreachable!("matched above"),
            })
        }
    }
}

/// Rejects a non-applicable optional edit position with a precise reason.
fn reject_inapplicable<T>(
    field: &'static str,
    edit: Option<&T>,
    families: &str,
) -> Result<(), ExtensionValidationError> {
    if edit.is_some() {
        return reject(
            field,
            format!("当前传输没有 {field} 字段；该字段只适用于 {families} 传输"),
        );
    }
    Ok(())
}

fn reject_inapplicable_map<T>(
    field: &'static str,
    edits: &BTreeMap<String, T>,
    families: &str,
) -> Result<(), ExtensionValidationError> {
    if !edits.is_empty() {
        return reject(
            field,
            format!("当前传输没有 {field} 字段；该字段只适用于 {families} 传输"),
        );
    }
    Ok(())
}
