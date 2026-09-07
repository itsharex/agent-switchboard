use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{table, value, Item, Table};

use crate::extensions::mcp::claude_patch::{render_json, render_json_entry};
use crate::extensions::mcp::EntryChange;

/// The four documented Claude skill visibility values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillOverrideValue {
    On,
    NameOnly,
    UserInvocableOnly,
    Off,
}

impl SkillOverrideValue {
    pub fn as_native(self) -> &'static str {
        match self {
            SkillOverrideValue::On => "on",
            SkillOverrideValue::NameOnly => "name-only",
            SkillOverrideValue::UserInvocableOnly => "user-invocable-only",
            SkillOverrideValue::Off => "off",
        }
    }

    pub fn parse_native(text: &str) -> Option<Self> {
        match text {
            "on" => Some(SkillOverrideValue::On),
            "name-only" => Some(SkillOverrideValue::NameOnly),
            "user-invocable-only" => Some(SkillOverrideValue::UserInvocableOnly),
            "off" => Some(SkillOverrideValue::Off),
            _ => None,
        }
    }
}

/// One skill visibility change in a Claude settings document.
pub fn apply_claude_skill_overrides(
    document: &str,
    changes: &[(String, Option<SkillOverrideValue>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut root = crate::adapter::claude::parse(document)?;
    let mut entry_changes = Vec::new();
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "设置文档根不是对象".to_string(),
            line: None,
        })?;
    let writes_anything = changes.iter().any(|(_, change)| change.is_some());
    if !root_object.contains_key("skillOverrides") && writes_anything {
        root_object.insert(
            "skillOverrides".to_string(),
            JsonValue::Object(JsonMap::new()),
        );
    }
    let Some(overrides) = root_object
        .get_mut("skillOverrides")
        .and_then(|value| value.as_object_mut())
    else {
        return Ok((render_json(&root), entry_changes));
    };
    for (name, change) in changes {
        let before = overrides.get(name).map(render_json_entry);
        match change {
            Some(value) => {
                overrides.insert(
                    name.clone(),
                    JsonValue::String(value.as_native().to_string()),
                );
                entry_changes.push(EntryChange {
                    pointer: format!("skillOverrides.{name}"),
                    before,
                    after: Some(format!("\"{}\"", value.as_native())),
                });
            }
            None => {
                if before.is_some() {
                    overrides.remove(name);
                    entry_changes.push(EntryChange {
                        pointer: format!("skillOverrides.{name}"),
                        before,
                        after: None,
                    });
                }
            }
        }
    }
    if overrides.is_empty() {
        root_object.remove("skillOverrides");
    }
    Ok((render_json(&root), entry_changes))
}

/// Restores one Claude skill override from the raw JSON value captured before
/// this application first changed it. This deliberately does not pass
/// through the four-state projection enum: a baseline must be able to put
/// back the exact native value it replaced.
pub fn apply_claude_skill_override_restore(
    document: &str,
    name: &str,
    original: Option<&str>,
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut root = crate::adapter::claude::parse(document)?;
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "设置文档根不是对象".to_string(),
            line: None,
        })?;
    let restored = original
        .map(|raw| {
            serde_json::from_str::<JsonValue>(raw).map_err(|error| crate::adapter::AdapterError {
                message: format!("备份的原始 Skill 覆盖规则无法解析：{error}"),
                line: None,
            })
        })
        .transpose()?;
    if restored.is_some() && !root_object.contains_key("skillOverrides") {
        root_object.insert(
            "skillOverrides".to_string(),
            JsonValue::Object(JsonMap::new()),
        );
    }
    let Some(overrides) = root_object
        .get_mut("skillOverrides")
        .and_then(|value| value.as_object_mut())
    else {
        return if restored.is_none() {
            Ok((render_json(&root), Vec::new()))
        } else {
            Err(crate::adapter::AdapterError {
                message: "skillOverrides 不是对象，无法恢复原始规则".to_string(),
                line: None,
            })
        };
    };
    let before = overrides.get(name).map(render_json_entry);
    let after = match restored {
        Some(value) => {
            overrides.insert(name.to_string(), value.clone());
            Some(serde_json::to_string(&value).expect("json value"))
        }
        None => {
            overrides.remove(name);
            None
        }
    };
    let mut changes = Vec::new();
    if before.is_some() || after.is_some() {
        changes.push(EntryChange {
            pointer: format!("skillOverrides.{name}"),
            before,
            after,
        });
    }
    if overrides.is_empty() {
        root_object.remove("skillOverrides");
    }
    Ok((render_json(&root), changes))
}

/// One Codex `[[skills.config]]` rule observed in a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSkillRule {
    pub path: String,
    pub enabled: bool,
}

/// Reads the Codex skill enable/disable rules.
pub fn read_codex_skill_rules(
    document: &str,
) -> Result<Vec<CodexSkillRule>, crate::adapter::AdapterError> {
    let doc = crate::adapter::codex::parse(document)?;
    let mut rules = Vec::new();
    let Some(rules_array) = doc
        .get("skills")
        .and_then(|item| item.get("config"))
        .and_then(|item| item.as_array_of_tables())
    else {
        return Ok(rules);
    };
    for rule in rules_array {
        let path = rule.get("path").and_then(|item| item.as_str());
        let enabled = rule.get("enabled").and_then(|item| item.as_bool());
        let Some(path) = path else {
            continue;
        };
        rules.push(CodexSkillRule {
            path: path.to_string(),
            enabled: enabled.unwrap_or(true),
        });
    }
    Ok(rules)
}

/// Writes Codex skill rules. Each change targets one rule by its canonical
/// document path; `Some(true/false)` writes the rule, `None` removes the
/// application-owned rule. Rules the application does not own are left
/// untouched unless the change replaces their exact path.
pub fn apply_codex_skill_rules(
    document: &str,
    changes: &[(String, Option<bool>)],
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let mut doc = crate::adapter::codex::parse(document)?;
    let mut entry_changes = Vec::new();
    let writes_anything = changes.iter().any(|(_, change)| change.is_some());
    if !doc.contains_key("skills") {
        if !writes_anything {
            return Ok((doc.to_string(), entry_changes));
        }
        doc["skills"] = table();
    }
    let Some(skills_table) = doc
        .get_mut("skills")
        .and_then(|item| item.as_table_like_mut())
    else {
        return Err(crate::adapter::AdapterError {
            message: "skills 不是表，无法写入技能规则".to_string(),
            line: None,
        });
    };
    if !matches!(skills_table.get("config"), Some(item) if item.is_array_of_tables()) {
        if !writes_anything {
            return Ok((doc.to_string(), entry_changes));
        }
        skills_table.insert(
            "config",
            Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    let Some(rules) = skills_table
        .get_mut("config")
        .and_then(|item| item.as_array_of_tables_mut())
    else {
        return Ok((doc.to_string(), entry_changes));
    };
    for (path, change) in changes {
        let position = rules.iter().position(|rule| {
            rule.get("path").and_then(|item| item.as_str()) == Some(path.as_str())
        });
        let before = position.map(|index| rules.iter().nth(index).expect("checked").to_string());
        match (change, position) {
            (Some(enabled), Some(index)) => {
                // Keep the existing table in place and rewrite only the
                // two fields this adapter owns. A native client may add
                // fields to the same rule; visibility changes must not
                // erase them.
                let slot = rules.iter_mut().nth(index).expect("checked");
                slot.insert("path", value(path.as_str()));
                slot.insert("enabled", value(*enabled));
                let after = slot.to_string();
                entry_changes.push(EntryChange {
                    pointer: format!("skills.config[path={path}]"),
                    before,
                    after: Some(after),
                });
            }
            (Some(enabled), None) => {
                let mut rule = Table::new();
                rule.insert("path", value(path.as_str()));
                rule.insert("enabled", value(*enabled));
                let after = rule.to_string();
                rules.push(rule);
                entry_changes.push(EntryChange {
                    pointer: format!("skills.config[path={path}]"),
                    before,
                    after: Some(after),
                });
            }
            (None, Some(index)) => {
                rules.remove(index);
                entry_changes.push(EntryChange {
                    pointer: format!("skills.config[path={path}]"),
                    before,
                    after: None,
                });
            }
            (None, None) => {}
        }
    }
    if rules.is_empty() {
        let skills_table = doc
            .get_mut("skills")
            .and_then(|item| item.as_table_like_mut());
        if let Some(skills_table) = skills_table {
            skills_table.remove("config");
            if skills_table.is_empty() {
                doc.remove("skills");
            }
        }
    }
    Ok((doc.to_string(), entry_changes))
}

/// Restores one Codex skill rule from the raw TOML table captured before this
/// application first changed it. The restore path does not re-render the rule
/// through the two-field projection, so native fields introduced by Codex or
/// the user return exactly as they were.
pub fn apply_codex_skill_rule_restore(
    document: &str,
    path: &str,
    original: Option<&str>,
) -> Result<(String, Vec<EntryChange>), crate::adapter::AdapterError> {
    let restored = original
        .map(|raw| {
            let wrapped = format!("[[skills.config]]\n{raw}");
            let parsed = crate::adapter::codex::parse(&wrapped)?;
            let rule = parsed
                .get("skills")
                .and_then(|item| item.get("config"))
                .and_then(|item| item.as_array_of_tables())
                .and_then(|rules| rules.iter().next())
                .cloned()
                .ok_or_else(|| crate::adapter::AdapterError {
                    message: "备份的原始 Codex Skill 规则无法解析".to_string(),
                    line: None,
                })?;
            if rule.get("path").and_then(|item| item.as_str()) != Some(path) {
                return Err(crate::adapter::AdapterError {
                    message: "备份的原始 Codex Skill 规则路径不匹配".to_string(),
                    line: None,
                });
            }
            Ok(rule)
        })
        .transpose()?;
    let mut doc = crate::adapter::codex::parse(document)?;
    if restored.is_some() && !doc.contains_key("skills") {
        doc["skills"] = table();
    }
    let Some(skills_table) = doc
        .get_mut("skills")
        .and_then(|item| item.as_table_like_mut())
    else {
        return if restored.is_none() {
            Ok((doc.to_string(), Vec::new()))
        } else {
            Err(crate::adapter::AdapterError {
                message: "skills 不是表，无法恢复原始规则".to_string(),
                line: None,
            })
        };
    };
    if !matches!(skills_table.get("config"), Some(item) if item.is_array_of_tables()) {
        if restored.is_none() {
            return Ok((doc.to_string(), Vec::new()));
        }
        skills_table.insert(
            "config",
            Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    let rules = skills_table
        .get_mut("config")
        .and_then(|item| item.as_array_of_tables_mut())
        .ok_or_else(|| crate::adapter::AdapterError {
            message: "skills.config 不是规则数组，无法恢复原始规则".to_string(),
            line: None,
        })?;
    let position = rules
        .iter()
        .position(|rule| rule.get("path").and_then(|item| item.as_str()) == Some(path));
    let before = position.map(|index| rules.iter().nth(index).expect("checked").to_string());
    let after = match (restored, position) {
        (Some(rule), Some(index)) => {
            let slot = rules.iter_mut().nth(index).expect("checked");
            *slot = rule;
            Some(slot.to_string())
        }
        (Some(rule), None) => {
            let after = rule.to_string();
            rules.push(rule);
            Some(after)
        }
        (None, Some(index)) => {
            rules.remove(index);
            None
        }
        (None, None) => None,
    };
    let mut changes = Vec::new();
    if before.is_some() || after.is_some() {
        changes.push(EntryChange {
            pointer: format!("skills.config[path={path}]"),
            before,
            after,
        });
    }
    if rules.is_empty() {
        let skills_table = doc
            .get_mut("skills")
            .and_then(|item| item.as_table_like_mut());
        if let Some(skills_table) = skills_table {
            skills_table.remove("config");
            if skills_table.is_empty() {
                doc.remove("skills");
            }
        }
    }
    Ok((doc.to_string(), changes))
}
