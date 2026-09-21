//! Client preference intent shares the configuration transaction and its recovery decision.
use super::*;
use asb_core::contracts::{ClientSettingsSnapshot, SettingsValues};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ClientSettingsIntent {
    before: SettingsValues,
    before_hash: String,
    after: SettingsValues,
    after_hash: String,
}

impl ClientSettingsIntent {
    pub(super) fn new(
        app: AppKind,
        before: &ClientSettingsSnapshot,
        after: &SettingsValues,
    ) -> Result<Self, String> {
        after.validate_client_settings(app).map_err(|error| error.to_string())?;
        let json = serde_json::to_string_pretty(after).map_err(|error| error.to_string())?;
        Ok(Self {
            before: before.settings.clone(),
            before_hash: before.settings_hash.clone(),
            after: after.clone(),
            after_hash: crate::config_store::content_revision(json.as_bytes()),
        })
    }

    fn apply(&self, state: &LocalState, app: AppKind, after: bool) -> Result<(), String> {
        self.before.validate_client_settings(app).map_err(|error| error.to_string())?;
        self.after.validate_client_settings(app).map_err(|error| error.to_string())?;
        let current = state.configuration().get_client_settings(app).map_err(|error| error.to_string())?;
        let desired = if after { &self.after } else { &self.before };
        if current.settings == *desired {
            return Ok(());
        }
        let expected = if after { &self.before_hash } else { &self.after_hash };
        if current.settings_hash != *expected {
            return Err("客户端设置在事务期间发生额外变化，保留事务等待恢复".into());
        }
        state.configuration().save_client_settings(app, desired.clone(), expected)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

pub(super) fn reconcile(state: &LocalState, intent: &SwitchIntent, after: bool) -> Result<(), String> {
    match &intent.client_settings {
        Some(settings) => settings.apply(state, intent.app, after),
        None => Ok(()),
    }
}

pub(super) fn capture_backup(
    state: &LocalState,
    intent: &SwitchIntent,
    backup: &asb_core::BackupRecord,
) -> Result<(), String> {
    let Some(settings) = &intent.client_settings else { return Ok(()); };
    if backup.app != intent.app || backup.target_path != intent.target
        || backup.content_hash != intent.before_hash || backup.target_existed != intent.before_existed {
        return Err("客户端设置事务与配置备份不匹配".into());
    }
    super::super::client_settings_backup::save(state, backup, &settings.before)
}

pub(in crate::commands) fn commit_client_settings(
    state: &LocalState,
    backup: &asb_core::BackupRecord,
) -> Result<(), String> {
    let intent = load(state)?.ok_or("缺少客户端设置事务")?;
    capture_backup(state, &intent, backup)?;
    reconcile(state, &intent, true)
}

pub(super) fn capture_current_backup(state: &LocalState, backup: &asb_core::BackupRecord) -> Result<(), String> {
    let saved = state.configuration().get_client_settings(backup.app).map_err(|error| error.to_string())?;
    super::super::client_settings_backup::save(state, backup, &saved.settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{ConfigValue, SettingValue};

    fn with_tips(mut settings: SettingsValues, enabled: bool) -> SettingsValues {
        settings.settings.insert("spinnerTipsEnabled".into(), SettingValue::Explicit {
            value: ConfigValue::Bool(enabled),
        });
        settings
    }

    #[test]
    fn interrupted_reset_finishes_and_compensates_saved_intent() {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().to_path_buf());
        let config = state.configuration();
        let initial = config.get_client_settings(AppKind::Claude).unwrap();
        let before = config.save_client_settings(
            AppKind::Claude, with_tips(initial.settings, true), &initial.settings_hash,
        ).unwrap();
        let defaults = asb_core::ownership::default_client_settings(AppKind::Claude);
        let intent = ClientSettingsIntent::new(AppKind::Claude, &before, &defaults).unwrap();

        intent.apply(&state, AppKind::Claude, true).unwrap();
        intent.apply(&state, AppKind::Claude, true).unwrap();
        assert_eq!(config.get_client_settings(AppKind::Claude).unwrap().settings, defaults);
        intent.apply(&state, AppKind::Claude, false).unwrap();
        intent.apply(&state, AppKind::Claude, false).unwrap();
        assert_eq!(config.get_client_settings(AppKind::Claude).unwrap().settings, before.settings);
    }

    #[test]
    fn recovery_refuses_unrelated_preference_changes() {
        let directory = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(directory.path().to_path_buf());
        let config = state.configuration();
        let before = config.get_client_settings(AppKind::Claude).unwrap();
        let intent = ClientSettingsIntent::new(
            AppKind::Claude, &before, &with_tips(before.settings.clone(), true),
        ).unwrap();
        let external = with_tips(before.settings.clone(), false);
        config.save_client_settings(AppKind::Claude, external.clone(), &before.settings_hash).unwrap();

        assert!(intent.apply(&state, AppKind::Claude, true).is_err());
        assert_eq!(config.get_client_settings(AppKind::Claude).unwrap().settings, external);
    }
}
