//! Typed catalogs for provider parameters and independently stored client
//! preferences. Provider parameters are saved only with their provider draft.

use super::error::{blocking, operation_error, state, store_error, CommandError};
use asb_core::contracts::{AppKind, ClientSettingsSnapshot, SettingsValues};
use asb_core::ownership::{
    self, ChoiceControl, OfficialSettingDisposition, SettingControl, SettingOwner,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// A catalog option available for a typed setting control.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingChoiceOption {
    pub value: String,
    pub label: String,
}

/// A redacted snapshot of the current client file. The renderer may edit this
/// display copy only through the manual-configuration executor, which restores
/// retained secret markers on the backend and never exposes the raw document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentClientConfiguration {
    pub app: AppKind,
    pub target: String,
    pub exists: bool,
    pub content: String,
    pub content_hash: String,
    pub syntax_ok: bool,
    pub syntax_error: Option<String>,
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
        let gate = app.state::<super::ConfigWriteGate>();
        let _guard = gate.lock().map_err(|error| CommandError::new("config-write-gate-unavailable", error))?;
        super::switching::ensure_profile_save_recovered(&app)?;
        state
            .configuration()
            .save_client_settings(target, settings, &expected_settings_hash)
            .map_err(|error| operation_error("client-settings-save-failed", error))
    })
    .await
}

/// Reads the actual client configuration and returns only a redacted display
/// snapshot. The renderer cannot use this result as a write target.
#[tauri::command]
pub async fn get_current_client_configuration(
    app: AppHandle,
    target: AppKind,
) -> Result<CurrentClientConfiguration, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let path = state
            .target(target)
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let (exists, raw) = match std::fs::read_to_string(&path) {
            Ok(content) => (true, content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
                false,
                match target { AppKind::Codex => String::new(), AppKind::Claude => "{}".to_string() },
            ),
            Err(_) => return Err(CommandError::new("client-configuration-unreadable", "无法读取真实客户端配置文件")),
        };
        let source = if raw.is_empty() && target == AppKind::Claude { "{}" } else { &raw };
        let syntax = asb_core::adapter::validate_syntax(target, source);
        let syntax_ok = syntax.is_ok();
        let syntax_error = syntax.err().map(|error| error.message);
        Ok(CurrentClientConfiguration {
            app: target,
            target: path.to_string_lossy().into_owned(),
            exists,
            content: syntax_ok.then(|| asb_switch::display_content(target, source)).unwrap_or_default(),
            content_hash: asb_switch::sha256_hex(&raw),
            syntax_ok,
            syntax_error,
        })
    }).await
}

/// Parses the dedicated Claude extra-configuration editor. Visual client
/// settings, provider settings, credentials, and extension fields are
/// rejected; the returned map is the complete ASB-managed extra contract.
#[tauri::command]
pub async fn parse_claude_extra_configuration(
    content: String,
) -> Result<asb_core::claude_common::Extra, CommandError> {
    blocking(move || {
        asb_core::claude_common::parse_extra(&content)
            .map_err(|error| CommandError::new("claude-extra-configuration-parse-failed", error))
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
    fn claude_extra_editor_accepts_only_extra_fields() {
        let parsed = tauri::async_runtime::block_on(parse_claude_extra_configuration(
            r#"{"permissions":{"allow":["Read"]}}"#.to_string(),
        ))
        .expect("extra configuration");
        assert_eq!(parsed["permissions"]["allow"], serde_json::json!(["Read"]));

        let error = tauri::async_runtime::block_on(parse_claude_extra_configuration(
            r#"{"spinnerTipsEnabled":true}"#.to_string(),
        ))
        .expect_err("visual setting must stay in the form");
        assert_eq!(error.code, "claude-extra-configuration-parse-failed");
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

}
