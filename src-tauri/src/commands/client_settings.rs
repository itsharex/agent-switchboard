//! Typed catalogs for provider parameters and independently stored client
//! preferences. Provider parameters are saved only with their provider draft.

use super::error::{blocking, operation_error, state, store_error, CommandError};
use asb_core::contracts::{AppKind, ClientSettingsPreview, ClientSettingsSnapshot, SettingsValues};
use asb_core::ownership::{
    self, ChoiceControl, OfficialSettingDisposition, SettingControl, SettingOwner,
};
use serde::Serialize;
use tauri::AppHandle;

/// A catalog option available for a typed setting control.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingChoiceOption {
    pub value: String,
    pub label: String,
}

/// One editor-safe projection of the ownership directory. The renderer never
/// receives arbitrary config keys: every parameter comes from its owner.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    pub key: String,
    pub label: String,
    pub group: String,
    /// `toggle`, `slider`, `segment`, or `model`.
    pub control: String,
    pub options: Vec<SettingChoiceOption>,
}

/// One official configuration family and its actual ownership boundary. This
/// gives the renderer an exhaustive directory without handing it arbitrary
/// file paths or a second write mechanism.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialSettingDirectoryEntry {
    pub title: String,
    pub paths: Vec<String>,
    /// `direct`, `separateModule`, or `preserveOnly`.
    pub disposition: String,
    pub detail: String,
}

/// Complete typed input required by the client settings page.
/// `settings` and `settings_hash` are application state only, not a rendered
/// client configuration candidate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSettingsEditor {
    pub app: AppKind,
    pub settings: SettingsValues,
    pub settings_hash: String,
    pub groups: Vec<String>,
    pub specs: Vec<SettingSpec>,
    pub directory: Vec<OfficialSettingDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderParametersCatalog {
    pub app: AppKind,
    pub defaults: SettingsValues,
    pub groups: Vec<String>,
    pub specs: Vec<SettingSpec>,
}

fn directory_catalog(target: AppKind) -> Vec<OfficialSettingDirectoryEntry> {
    ownership::official_setting_directory(target)
        .into_iter()
        .map(|entry| OfficialSettingDirectoryEntry {
            title: entry.title.to_string(),
            paths: std::iter::once(entry.path)
                .chain(entry.related_paths.iter().copied())
                .map(str::to_string)
                .collect(),
            disposition: match entry.disposition {
                OfficialSettingDisposition::Direct => "direct",
                OfficialSettingDisposition::SeparateModule => "separateModule",
                OfficialSettingDisposition::PreserveOnly => "preserveOnly",
            }
            .to_string(),
            detail: entry.detail.to_string(),
        })
        .collect()
}

fn editor_catalog(target: AppKind, owner: SettingOwner) -> Vec<SettingSpec> {
    ownership::setting_specs(target)
        .into_iter()
        .filter(|spec| spec.owner == owner && !matches!(spec.control, SettingControl::None))
        .map(|spec| {
            let (control, options) = match spec.control {
                SettingControl::Toggle => ("toggle".to_string(), vec![]),
                SettingControl::Choice { presentation } => (
                    match presentation {
                        ChoiceControl::Slider => "slider",
                        ChoiceControl::Segment => "segment",
                    }
                    .to_string(),
                    spec.allowed_values
                        .iter()
                        .map(|option| SettingChoiceOption {
                            value: option.value.to_string(),
                            label: option.label.to_string(),
                        })
                        .collect(),
                ),
                SettingControl::ModelPicker => ("model".to_string(), vec![]),
                SettingControl::None => unreachable!("editor controls are filtered above"),
            };
            SettingSpec {
                key: spec.key.to_string(),
                label: spec
                    .label
                    .expect("editable setting must have a visible label")
                    .to_string(),
                group: spec
                    .group
                    .expect("editable setting must have an editor group")
                    .to_string(),
                control,
                options,
            }
        })
        .collect()
}

fn editor_groups(specs: &[SettingSpec]) -> Vec<String> {
    let mut groups = Vec::new();
    for spec in specs {
        if !groups.contains(&spec.group) {
            groups.push(spec.group.clone());
        }
    }
    groups
}

fn editor_from_snapshot(target: AppKind, snapshot: ClientSettingsSnapshot) -> ClientSettingsEditor {
    let specs = editor_catalog(target, SettingOwner::Client);
    ClientSettingsEditor {
        app: target,
        settings_hash: snapshot.settings_hash,
        settings: snapshot.settings,
        groups: editor_groups(&specs),
        specs,
        directory: directory_catalog(target),
    }
}

#[tauri::command]
pub fn get_provider_parameters_catalog(target: AppKind) -> ProviderParametersCatalog {
    let specs = editor_catalog(target, SettingOwner::Provider);
    ProviderParametersCatalog {
        app: target,
        defaults: ownership::default_provider_parameters(target),
        groups: editor_groups(&specs),
        specs,
    }
}

/// Reads the application-owned parameter values and the catalog that can edit
/// them. This command deliberately does not access the real client
/// configuration.
#[tauri::command]
pub async fn get_client_settings_editor(
    app: AppHandle,
    target: AppKind,
) -> Result<ClientSettingsEditor, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        state
            .configuration()
            .get_client_settings(target)
            .map(|snapshot| editor_from_snapshot(target, snapshot))
            .map_err(store_error)
    })
    .await
}

/// Replaces client settings after an optimistic revision check.
/// Saving here does not itself modify either real client config file; the
/// supplier switch path remains the only projection writer. A previously
/// confirmed interrupted profile save is recovered before this write.
#[tauri::command]
pub async fn save_client_settings(
    app: AppHandle,
    target: AppKind,
    settings: SettingsValues,
    expected_settings_hash: String,
) -> Result<ClientSettingsSnapshot, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        super::switching::ensure_profile_save_recovered(&app)?;
        state
            .configuration()
            .save_client_settings(target, settings, &expected_settings_hash)
            .map_err(|error| operation_error("client-settings-save-failed", error))
    })
    .await
}

/// Renders the editor's current draft as an application-owned client-settings
/// fragment. It is pure: no client file is read or written, and no provider
/// data can enter this preview.
#[tauri::command]
pub async fn preview_client_settings(
    target: AppKind,
    settings: SettingsValues,
) -> Result<ClientSettingsPreview, CommandError> {
    blocking(move || {
        let content =
            asb_core::adapter::render_client_settings(target, &settings).map_err(|error| {
                CommandError::new("client-settings-preview-failed", error.to_string())
            })?;
        Ok(ClientSettingsPreview {
            app: target,
            target: format!("{} 客户端配置片段", target.config_label()),
            content,
        })
    })
    .await
}

/// Parses an editable client-settings fragment back into the application
/// preference contract. This is pure: it cannot read or write a real client
/// configuration file, and provider or host-owned keys are rejected.
#[tauri::command]
pub async fn parse_client_settings(
    target: AppKind,
    content: String,
) -> Result<SettingsValues, CommandError> {
    blocking(move || {
        asb_core::adapter::parse_client_settings(target, &content)
            .map_err(|error| CommandError::new("client-settings-parse-failed", error.to_string()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogs_expose_only_controls_belonging_to_their_scope() {
        for app in [AppKind::Codex, AppKind::Claude] {
            for owner in [SettingOwner::Client, SettingOwner::Provider] {
                let catalog = editor_catalog(app, owner);
                assert!(!catalog.is_empty());
                for setting in &catalog {
                    assert_eq!(ownership::owner_for(app, &setting.key), owner);
                    assert!(!setting.label.is_empty());
                    assert!(!setting.group.is_empty());
                    assert!(matches!(
                        setting.control.as_str(),
                        "toggle" | "slider" | "segment" | "model"
                    ));
                    if matches!(setting.control.as_str(), "toggle" | "model") {
                        assert!(setting.options.is_empty());
                    } else {
                        assert!(!setting.options.is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn provider_catalog_and_required_defaults_have_the_same_keys() {
        for app in [AppKind::Codex, AppKind::Claude] {
            let catalog = get_provider_parameters_catalog(app);
            let keys = catalog
                .specs
                .iter()
                .map(|spec| spec.key.clone())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(keys, catalog.defaults.settings.keys().cloned().collect());
            assert!(!keys.contains("approval_policy"));
        }
    }

    #[test]
    fn editor_has_no_client_file_projection_data() {
        let editor = editor_from_snapshot(
            AppKind::Codex,
            ClientSettingsSnapshot {
                settings: ownership::default_client_settings(AppKind::Codex),
                settings_hash: "revision".to_string(),
            },
        );
        let json = serde_json::to_value(editor).expect("editor serializes");
        assert!(json.get("target").is_none());
        assert!(json.get("content").is_none());
        assert!(json.get("preview").is_none());
        // The official directory may name a path family and its ownership
        // boundary, but it never carries a rendered client-file candidate or
        // patch instruction.
        let text = serde_json::to_string(&json).unwrap();
        for forbidden in ["Leave", "Remove", "backupPath", "renderedHash"] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn preview_renders_only_the_client_settings_fragment() {
        let settings = ownership::default_client_settings(AppKind::Codex);
        let preview =
            tauri::async_runtime::block_on(preview_client_settings(AppKind::Codex, settings))
                .expect("preview");
        assert_eq!(preview.app, AppKind::Codex);
        assert!(preview.target.contains("config.toml"));
        assert!(preview.content.contains("所有客户端设置均为自动"));
    }

    #[test]
    fn parser_returns_a_complete_client_draft_without_touching_client_files() {
        let parsed = tauri::async_runtime::block_on(parse_client_settings(
            AppKind::Claude,
            r#"{"spinnerTipsEnabled":true}"#.to_string(),
        ))
        .expect("parse");
        assert_eq!(
            parsed.settings["spinnerTipsEnabled"],
            asb_core::contracts::SettingValue::Explicit {
                value: asb_core::contracts::ConfigValue::Bool(true),
            }
        );
        assert!(parsed.settings.contains_key("autoScrollEnabled"));
    }
}
