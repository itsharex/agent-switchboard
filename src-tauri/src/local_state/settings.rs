use crate::runtime_log::RuntimeLogLevel;

/// App-runtime preference: controls what a user-visible close request does.
/// It never belongs to Codex or Claude Code configuration files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    HideToTray,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotionPreference {
    System,
    Reduce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupPage {
    Providers,
    LastVisited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePage {
    Providers,
    ClientConfiguration,
    Extensions,
    Sessions,
    Usage,
    Settings,
}

/// Bundled web font shipped with the app; also the interface-font default.
pub(crate) const DEFAULT_INTERFACE_FONT: &str = "Noto Sans SC";

/// Application-owned desktop preferences, stored separately from configuration
/// data. This is a strict complete current contract.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    pub close_behavior: CloseBehavior,
    pub theme: ThemePreference,
    pub motion: MotionPreference,
    pub always_on_top: bool,
    pub launch_at_login: bool,
    /// Keeps the main window hidden at startup; the app starts in the tray.
    pub start_minimized: bool,
    pub hardware_acceleration: bool,
    pub interface_font: String,
    pub interface_scale: u16,
    /// Empty disables the shortcut; nonempty values use the canonical chord format.
    pub global_shortcut: String,
    pub startup_page: StartupPage,
    pub runtime_log_level: RuntimeLogLevel,
    /// Provider ids whose usage panel stays collapsed.
    pub collapsed_usage_ids: Vec<String>,
}

impl AppSettings {
    /// A font name is used verbatim as a CSS font-family value, so it must be
    /// a plain non-empty name without quotes or control characters.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if ![90, 100, 110, 125].contains(&self.interface_scale) {
            return Err("界面缩放须为 90%、100%、110% 或 125%".to_string());
        }
        crate::desktop_shortcut::parse_shortcut(&self.global_shortcut)?;
        let valid = self.interface_font.trim().len() == self.interface_font.len()
            && !self.interface_font.is_empty()
            && self.interface_font.len() <= 64
            && !self
                .interface_font
                .chars()
                .any(|character| character.is_control() || matches!(character, '"' | '\'' | '\\'));
        if valid {
            Ok(())
        } else {
            Err("界面字体名称无效：须为非空字体名，且不含首尾空格、引号或控制字符".to_string())
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::HideToTray,
            theme: ThemePreference::System,
            motion: MotionPreference::System,
            always_on_top: false,
            launch_at_login: false,
            start_minimized: false,
            hardware_acceleration: true,
            interface_font: DEFAULT_INTERFACE_FONT.to_string(),
            interface_scale: 100,
            global_shortcut: String::new(),
            startup_page: StartupPage::Providers,
            runtime_log_level: RuntimeLogLevel::Info,
            collapsed_usage_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_preferences_have_one_complete_current_contract() {
        let settings = AppSettings::default();
        assert!(settings.validate().is_ok());
        assert_eq!(settings.interface_scale, 100);
        assert_eq!(settings.global_shortcut, "");
        assert_eq!(settings.startup_page, StartupPage::Providers);
        let current = serde_json::to_value(&settings).unwrap();
        for field in ["interfaceScale", "globalShortcut", "startupPage"] {
            let mut missing = current.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<AppSettings>(missing).is_err(), "{field} must be present");
        }
        let mut unknown = current;
        unknown["zoom"] = serde_json::json!(100);
        assert!(serde_json::from_value::<AppSettings>(unknown).is_err());
    }

    #[test]
    fn only_displayed_scale_choices_are_valid() {
        for scale in [90, 100, 110, 125] {
            let settings = AppSettings { interface_scale: scale, ..AppSettings::default() };
            assert!(settings.validate().is_ok());
        }
        for scale in [0, 95, 126, u16::MAX] {
            let settings = AppSettings { interface_scale: scale, ..AppSettings::default() };
            assert!(settings.validate().is_err());
        }
    }
}

/// Connection coordinates for a user-owned Supabase project. The project
/// publishable key identifies a public desktop client; authentication and the
/// separate cloud-backup password are intentionally never written to disk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloudBackupSettings {
    pub project_url: String,
    pub publishable_key: String,
    pub email: String,
}

impl CloudBackupSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let project_url = self.project_url.trim();
        let publishable_key = self.publishable_key.trim();
        let email = self.email.trim();
        let valid_url = project_url.starts_with("https://")
            && project_url.len() == self.project_url.len()
            && project_url.len() > "https://".len()
            && !project_url.contains(['?', '#', ' '])
            && !project_url.chars().any(char::is_control)
            && !project_url.ends_with('/');
        if !valid_url {
            return Err("Supabase 项目地址必须是无尾随斜杠的 https URL".to_string());
        }
        if publishable_key.is_empty()
            || publishable_key.len() != self.publishable_key.len()
            || publishable_key.len() > 2048
            || publishable_key.chars().any(char::is_control)
        {
            return Err("Supabase publishable key 无效".to_string());
        }
        if email.is_empty()
            || email.len() != self.email.len()
            || email.len() > 320
            || !email.contains('@')
            || email.chars().any(char::is_control)
        {
            return Err("项目 Auth 登录邮箱无效".to_string());
        }
        Ok(())
    }
}
