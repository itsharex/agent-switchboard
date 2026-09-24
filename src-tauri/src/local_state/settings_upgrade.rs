use super::{AppSettings, LocalState};
use super::settings::{CloseBehavior, ThemePreference, MotionPreference, StartupPage, LanguagePreference};
use crate::runtime_log::RuntimeLogLevel;
use std::{fs, io::ErrorKind};

impl LocalState {
    /// The startup-only conversion preserves every shipped preference. Runtime
    /// reads remain strict and a failed rewrite never masquerades as success.
    pub(crate) fn upgrade_app_settings(&self) -> Result<(), String> {
        let text = match fs::read_to_string(self.settings_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        match serde_json::from_str::<AppSettings>(&text) {
            Ok(settings) => settings.validate().map_err(|error| error.to_string()),
            Err(current_error) => {
                let settings = serde_json::from_str::<LegacyAppSettings>(&text)
                    .map_err(|_| current_error.to_string())?
                    .upgrade();
                self.set_app_settings(&settings)
            }
        }
    }
}

/// The complete shipped v0.2 preference structure without the language
/// preference. It exists only for the one-time startup upgrade: a file that
/// parses as this exact shape is converted to the current contract with the
/// interface language pinned to Chinese, then atomically rewritten. Any other
/// drift (unknown, missing, or invalid fields) stays in the refuse-and-repair
/// path, so corruption is never mistaken for an older version.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyAppSettings {
    pub close_behavior: CloseBehavior,
    pub theme: ThemePreference,
    pub motion: MotionPreference,
    pub always_on_top: bool,
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub hardware_acceleration: bool,
    pub interface_font: String,
    pub interface_scale: u16,
    pub global_shortcut: String,
    pub startup_page: StartupPage,
    pub runtime_log_level: RuntimeLogLevel,
    pub expanded_usage_ids: Vec<String>,
}

impl LegacyAppSettings {
    /// Existing users keep the interface they already saw: Chinese.
    pub(crate) fn upgrade(self) -> AppSettings {
        AppSettings {
            language: LanguagePreference::ZhCn,
            close_behavior: self.close_behavior,
            theme: self.theme,
            motion: self.motion,
            always_on_top: self.always_on_top,
            launch_at_login: self.launch_at_login,
            start_minimized: self.start_minimized,
            hardware_acceleration: self.hardware_acceleration,
            interface_font: self.interface_font,
            interface_scale: self.interface_scale,
            global_shortcut: self.global_shortcut,
            startup_page: self.startup_page,
            runtime_log_level: self.runtime_log_level,
            expanded_usage_ids: self.expanded_usage_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_preferences_upgrade_with_every_preference_kept_and_chinese_language() {
        // Any extra field disqualifies the legacy shape: corruption keeps the
        // refuse-and-repair path instead of upgrading.
        let mut corrupt = serde_json::to_value(AppSettings::default()).unwrap();
        corrupt.as_object_mut().unwrap().remove("language");
        corrupt["zoom"] = serde_json::json!(100);
        assert!(serde_json::from_value::<LegacyAppSettings>(corrupt).is_err());
        let upgraded = serde_json::from_str::<LegacyAppSettings>(
            r#"{
                "closeBehavior": "exit", "theme": "dark", "motion": "reduce",
                "alwaysOnTop": true, "launchAtLogin": true, "startMinimized": true,
                "hardwareAcceleration": false, "interfaceFont": "Segoe UI",
                "interfaceScale": 110, "globalShortcut": "control+KeyK",
                "startupPage": "lastVisited", "runtimeLogLevel": "warn",
                "expandedUsageIds": ["p1"]
            }"#,
        )
        .unwrap()
        .upgrade();
        assert_eq!(upgraded.language, LanguagePreference::ZhCn);
        assert_eq!(upgraded.close_behavior, CloseBehavior::Exit);
        assert_eq!(upgraded.theme, ThemePreference::Dark);
        assert_eq!(upgraded.interface_scale, 110);
        assert_eq!(upgraded.expanded_usage_ids, vec!["p1".to_string()]);
        assert!(upgraded.validate().is_ok());
        // The current contract is the only daily shape: a file carrying
        // `language` never parses as the legacy structure.
        assert!(serde_json::from_str::<LegacyAppSettings>(&serde_json::to_string(&AppSettings::default()).unwrap()).is_err());
    }

    #[test]
    fn invalid_legacy_preferences_are_not_rewritten() {
        let root = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(root.path().to_path_buf());
        let mut invalid = serde_json::to_value(AppSettings::default()).unwrap();
        invalid.as_object_mut().unwrap().remove("language");
        invalid["interfaceScale"] = serde_json::json!(95);
        let original = serde_json::to_vec(&invalid).unwrap();
        fs::write(state.settings_path(), &original).unwrap();
        assert!(state.upgrade_app_settings().is_err());
        assert_eq!(fs::read(state.settings_path()).unwrap(), original);
        assert!(state.get_app_settings().is_err());
    }

    #[test]
    fn current_settings_are_validated_without_a_rewrite() {
        let root = tempfile::tempdir().unwrap();
        let state = LocalState::from_root(root.path().to_path_buf());
        let original = serde_json::to_vec(&AppSettings::default()).unwrap();
        fs::write(state.settings_path(), &original).unwrap();
        state.upgrade_app_settings().unwrap();
        assert_eq!(fs::read(state.settings_path()).unwrap(), original);
    }
}
