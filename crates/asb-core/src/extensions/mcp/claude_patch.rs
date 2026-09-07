use std::collections::BTreeMap;

use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::extensions::mcp::observe::claude_modeled_fields_for;
use crate::extensions::mcp::render::ClaudeServerRender;
use crate::extensions::mcp::EntryChange;

/// Applies server patches to a Claude document that carries a top-level
/// `mcpServers` object (user scope) or receives one. Other document content
/// is preserved; formatting is re-rendered like the provider adapter.
pub fn apply_claude_user_server_patches(
    document: &str,
    patches: &[(String, Option<ClaudeServerRender>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    apply_claude_server_patches_at(document, &["mcpServers"], patches)
}

/// Applies server patches to a project-shared `.mcp.json` document.
pub fn apply_claude_project_server_patches(
    document: &str,
    patches: &[(String, Option<ClaudeServerRender>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    apply_claude_server_patches_at(document, &["mcpServers"], patches)
}

/// Applies server patches to a Claude user document's private project
/// section: `projects.<absolute path>.mcpServers`.
pub fn apply_claude_project_private_server_patches(
    document: &str,
    project_path: &str,
    patches: &[(String, Option<ClaudeServerRender>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    apply_claude_server_patches_at(document, &["projects", project_path, "mcpServers"], patches)
}

fn apply_claude_server_patches_at(
    document: &str,
    path: &[&str],
    patches: &[(String, Option<ClaudeServerRender>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut root = crate::adapter::claude::parse(document)?;
    let mut changes = Vec::new();
    let writes_anything = patches.iter().any(|(_, patch)| patch.is_some());
    // Walk to the target object, creating intermediate objects only when a
    // patch will actually write.
    let mut node: &mut JsonValue = &mut root;
    for segment in path {
        if !node.is_object() {
            if !writes_anything {
                return Ok((render_json(&root), changes));
            }
            *node = JsonValue::Object(JsonMap::new());
        }
        let object = node.as_object_mut().expect("object enforced above");
        let next = object
            .entry(segment.to_string())
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        node = next;
    }
    if !node.is_object() {
        if !writes_anything {
            return Ok((render_json(&root), changes));
        }
        *node = JsonValue::Object(JsonMap::new());
    }
    let servers = node.as_object_mut().expect("object enforced above");
    for (key, patch) in patches {
        let before = servers.get(key).map(render_json_entry);
        match patch {
            Some(render) => {
                let mut after = claude_render_value(render);
                // Carry over host-owned unknown fields from the existing
                // entry; modeled fields of the old shape are ours and get
                // rewritten by the new render.
                if let Some(existing) = servers.get(key).and_then(|value| value.as_object()) {
                    let modeled = claude_modeled_fields_for(existing);
                    if let Some(new_object) = after.as_object_mut() {
                        for (name, value) in existing {
                            if !modeled.contains(&name.as_str()) && !new_object.contains_key(name) {
                                new_object.insert(name.clone(), value.clone());
                            }
                        }
                    }
                }
                servers.insert(key.clone(), after);
                changes.push(EntryChange {
                    pointer: format!("{}.{}", path.join("."), key),
                    before,
                    after: Some(serde_json::to_string(&servers[key]).expect("json value")),
                });
            }
            None => {
                if before.is_some() {
                    servers.remove(key);
                    changes.push(EntryChange {
                        pointer: format!("{}.{}", path.join("."), key),
                        before,
                        after: None,
                    });
                }
            }
        }
    }
    Ok((render_json(&root), changes))
}

pub(super) fn render_json(root: &JsonValue) -> String {
    serde_json::to_string_pretty(root).unwrap_or_else(|_| "{}".to_string())
}

/// Restores one object entry captured in a baseline. `original` is the
/// parsed JSON of the previous entry, absent when it did not exist. Used by
/// restore planning; the change list follows the same shape as patches.
pub fn apply_claude_entry_restore(
    document: &str,
    pointer_path: &[&str],
    key: &str,
    original: Option<&JsonValue>,
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    // Restoration cannot go through the typed render: the original entry is
    // arbitrary JSON. Parse and set it directly.
    let mut root = crate::adapter::claude::parse(document)?;
    let mut changes = Vec::new();
    let mut node: &mut JsonValue = &mut root;
    for segment in pointer_path {
        if !node.is_object() {
            return Err(crate::adapter::AdapterError {
                message: format!("{} 不是对象，无法恢复条目", segment),
                line: None,
            });
        }
        let object = node.as_object_mut().expect("checked");
        let next = object
            .entry(segment.to_string())
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        node = next;
    }
    if !node.is_object() {
        return Err(crate::adapter::AdapterError {
            message: "目标位置不是对象，无法恢复条目".to_string(),
            line: None,
        });
    }
    let object = node.as_object_mut().expect("checked");
    let before = object.get(key).map(render_json_entry);
    match original {
        Some(value) => {
            object.insert(key.to_string(), value.clone());
            changes.push(EntryChange {
                pointer: format!("{}.{}", pointer_path.join("."), key),
                before,
                after: Some(serde_json::to_string(value).expect("json value")),
            });
        }
        None => {
            if before.is_some() {
                object.remove(key);
                changes.push(EntryChange {
                    pointer: format!("{}.{}", pointer_path.join("."), key),
                    before,
                    after: None,
                });
            }
        }
    }
    Ok((render_json(&root), changes))
}

fn claude_render_value(render: &ClaudeServerRender) -> JsonValue {
    let mut object = JsonMap::new();
    match render {
        ClaudeServerRender::Stdio { command, args, env } => {
            object.insert("type".to_string(), JsonValue::String("stdio".to_string()));
            object.insert("command".to_string(), JsonValue::String(command.clone()));
            if !args.is_empty() {
                object.insert(
                    "args".to_string(),
                    JsonValue::Array(args.iter().cloned().map(JsonValue::String).collect()),
                );
            }
            if !env.is_empty() {
                object.insert("env".to_string(), to_json_map(env));
            }
        }
        ClaudeServerRender::Http { url, headers } => {
            object.insert("type".to_string(), JsonValue::String("http".to_string()));
            object.insert("url".to_string(), JsonValue::String(url.clone()));
            if !headers.is_empty() {
                object.insert("headers".to_string(), to_json_map(headers));
            }
        }
        ClaudeServerRender::Sse { url, headers } => {
            object.insert("type".to_string(), JsonValue::String("sse".to_string()));
            object.insert("url".to_string(), JsonValue::String(url.clone()));
            if !headers.is_empty() {
                object.insert("headers".to_string(), to_json_map(headers));
            }
        }
        ClaudeServerRender::Ws { url, headers } => {
            object.insert("type".to_string(), JsonValue::String("ws".to_string()));
            object.insert("url".to_string(), JsonValue::String(url.clone()));
            if !headers.is_empty() {
                object.insert("headers".to_string(), to_json_map(headers));
            }
        }
    }
    JsonValue::Object(object)
}

fn to_json_map(map: &BTreeMap<String, String>) -> JsonValue {
    let mut object = JsonMap::new();
    for (name, val) in map {
        object.insert(name.clone(), JsonValue::String(val.clone()));
    }
    JsonValue::Object(object)
}

pub(super) fn render_json_entry(value: &JsonValue) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "".to_string())
}

/// Returns whether one exact server key is already disabled for a Claude
/// project. Existing malformed containers are rejected instead of being
/// replaced, because a visibility operation must never take ownership of an
/// unrelated native structure.
pub fn claude_project_disabled_member_present(
    document: &str,
    project_path: &str,
    member: &str,
) -> Result<bool, crate::adapter::AdapterError> {
    let root = crate::adapter::claude::parse(document)?;
    let root_object = root
        .as_object()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "用户文档根不是对象".to_string(),
            line: None,
        })?;
    let Some(projects) = root_object.get("projects") else {
        return Ok(false);
    };
    let projects = projects
        .as_object()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "projects 不是对象，无法读取项目 MCP 停用状态".to_string(),
            line: None,
        })?;
    let Some(project) = projects.get(project_path) else {
        return Ok(false);
    };
    let project = project
        .as_object()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "projects 条目不是对象，无法读取项目 MCP 停用状态".to_string(),
            line: None,
        })?;
    let Some(disabled) = project.get("disabledMcpServers") else {
        return Ok(false);
    };
    let members = disabled
        .as_array()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "disabledMcpServers 不是数组，无法读取项目 MCP 停用状态".to_string(),
            line: None,
        })?;
    Ok(members
        .iter()
        .any(|existing| existing.as_str() == Some(member)))
}

/// Adds or removes one member in a Claude project's `disabledMcpServers`
/// array inside the user document. The array is created only when adding,
/// and empty shells created for the change are cleaned up afterwards.
pub fn apply_claude_project_disabled_members(
    document: &str,
    project_path: &str,
    additions: &[String],
    removals: &[String],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut root = crate::adapter::claude::parse(document)?;
    let mut changes = Vec::new();
    let root_object = root
        .as_object()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "用户文档根不是对象".to_string(),
            line: None,
        })?;
    if let Some(projects) = root_object.get("projects") {
        let projects = projects
            .as_object()
            .ok_or_else(|| crate::adapter::AdapterError {
                message: "projects 不是对象，无法写入项目 MCP 停用状态".to_string(),
                line: None,
            })?;
        if let Some(project) = projects.get(project_path) {
            let project = project
                .as_object()
                .ok_or_else(|| crate::adapter::AdapterError {
                    message: "projects 条目不是对象，无法写入项目 MCP 停用状态".to_string(),
                    line: None,
                })?;
            if let Some(disabled) = project.get("disabledMcpServers") {
                if !disabled.is_array() {
                    return Err(crate::adapter::AdapterError {
                        message: "disabledMcpServers 不是数组，无法写入项目 MCP 停用状态"
                            .to_string(),
                        line: None,
                    });
                }
            } else if additions.is_empty() {
                return Ok((render_json(&root), changes));
            }
        } else if additions.is_empty() {
            return Ok((render_json(&root), changes));
        }
    } else if additions.is_empty() {
        return Ok((render_json(&root), changes));
    }

    if !root
        .as_object()
        .expect("root object checked above")
        .contains_key("projects")
    {
        root.as_object_mut()
            .expect("root object checked above")
            .insert("projects".to_string(), JsonValue::Object(JsonMap::new()));
    }
    let project_missing = root
        .as_object()
        .expect("root object checked above")
        .get("projects")
        .and_then(JsonValue::as_object)
        .expect("projects shape checked above")
        .get(project_path)
        .is_none();
    if project_missing {
        root.as_object_mut()
            .expect("root object checked above")
            .get_mut("projects")
            .and_then(JsonValue::as_object_mut)
            .expect("projects shape checked above")
            .insert(project_path.to_string(), JsonValue::Object(JsonMap::new()));
    }
    let has_disabled_members = root
        .as_object()
        .expect("root object checked above")
        .get("projects")
        .and_then(JsonValue::as_object)
        .and_then(|projects| projects.get(project_path))
        .and_then(JsonValue::as_object)
        .expect("project shape checked above")
        .contains_key("disabledMcpServers");
    if !has_disabled_members {
        root.as_object_mut()
            .expect("root object checked above")
            .get_mut("projects")
            .and_then(JsonValue::as_object_mut)
            .expect("projects shape checked above")
            .get_mut(project_path)
            .and_then(JsonValue::as_object_mut)
            .expect("project shape checked above")
            .insert(
                "disabledMcpServers".to_string(),
                JsonValue::Array(Vec::new()),
            );
    }
    let pointer = format!("projects.{project_path}.disabledMcpServers");
    let collection_emptied = {
        let members = root
            .as_object_mut()
            .expect("root object checked above")
            .get_mut("projects")
            .and_then(JsonValue::as_object_mut)
            .expect("projects shape checked above")
            .get_mut(project_path)
            .and_then(JsonValue::as_object_mut)
            .expect("project shape checked above")
            .get_mut("disabledMcpServers")
            .and_then(JsonValue::as_array_mut)
            .expect("disabled members shape checked above");
        let snapshot = |members: &[JsonValue]| {
            serde_json::to_string(&JsonValue::Array(members.to_vec())).expect("json array")
        };
        for member in additions {
            if !members
                .iter()
                .any(|existing| existing.as_str() == Some(member))
            {
                let before = snapshot(members);
                members.push(JsonValue::String(member.clone()));
                changes.push(EntryChange {
                    pointer: pointer.clone(),
                    before: Some(before),
                    after: Some(snapshot(members)),
                });
            }
        }
        for member in removals {
            let before = snapshot(members);
            let length_before = members.len();
            members.retain(|existing| existing.as_str() != Some(member));
            if members.len() != length_before {
                changes.push(EntryChange {
                    pointer: pointer.clone(),
                    before: Some(before),
                    after: Some(snapshot(members)),
                });
            }
        }
        members.is_empty()
    };
    if collection_emptied && !changes.is_empty() {
        let projects_object = root
            .as_object_mut()
            .expect("root object checked above")
            .get_mut("projects")
            .and_then(|value| value.as_object_mut())
            .expect("projects shape checked above");
        let project_object = projects_object
            .get_mut(project_path)
            .and_then(JsonValue::as_object_mut)
            .expect("project shape checked above");
        project_object.remove("disabledMcpServers");
        let project_empty = project_object.is_empty();
        if project_empty {
            projects_object.remove(project_path);
        }
        if projects_object.is_empty() {
            root.as_object_mut()
                .expect("root object checked above")
                .remove("projects");
        }
    }
    Ok((render_json(&root), changes))
}
