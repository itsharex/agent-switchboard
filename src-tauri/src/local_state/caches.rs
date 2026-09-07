use super::{AppSettings, CloudBackupSettings, LocalState};
use crate::codex_official_quota::CodexQuotaBaseline;
use crate::codex_reset::CodexResetStatus;
use crate::model_usage_cache::ModelUsageCache;
use crate::usage_cache::UsageCache;
use asb_core::discovery::DiscoveryReport;
use std::fs;
use uuid::Uuid;

impl LocalState {
    pub fn get_app_settings(&self) -> Result<AppSettings, String> {
        match fs::read_to_string(self.settings_path()) {
            Ok(text) => {
                let settings = serde_json::from_str::<AppSettings>(&text)
                    .map_err(|_| "应用设置格式无效".to_string())?;
                settings
                    .validate()
                    .map_err(|_| "应用设置格式无效".to_string())?;
                Ok(settings)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(AppSettings::default())
            }
            Err(_) => Err("应用设置不可读".to_string()),
        }
    }

    pub fn set_app_settings(&self, settings: &AppSettings) -> Result<(), String> {
        settings.validate()?;
        let content =
            serde_json::to_string_pretty(settings).map_err(|_| "应用设置序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self.root.join(format!("settings.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content).map_err(|_| "无法写入应用设置临时文件".to_string())?;
        if fs::rename(&temporary, self.settings_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存应用设置".to_string());
        }
        Ok(())
    }

    /// One-click repair for an unreadable settings file: replaces it with
    /// validated defaults through the same atomic write as a normal save. A
    /// readable settings file is refused, so repair can never silently
    /// discard healthy preferences.
    pub fn repair_app_settings(&self) -> Result<AppSettings, String> {
        if self.get_app_settings().is_ok() {
            return Err("应用设置当前可读，无需修复".to_string());
        }
        let defaults = AppSettings::default();
        self.set_app_settings(&defaults)?;
        Ok(defaults)
    }

    /// The cloud destination is optional. Reading an absent file never
    /// creates state, matching the other app-owned optional snapshots.
    pub fn get_cloud_backup_settings(&self) -> Result<Option<CloudBackupSettings>, String> {
        let text = match fs::read_to_string(self.cloud_backup_settings_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("云端备份设置不可读".to_string()),
        };
        let settings: CloudBackupSettings =
            serde_json::from_str(&text).map_err(|_| "云端备份设置格式无效".to_string())?;
        settings
            .validate()
            .map_err(|_| "云端备份设置格式无效".to_string())?;
        Ok(Some(settings))
    }

    pub fn set_cloud_backup_settings(&self, settings: &CloudBackupSettings) -> Result<(), String> {
        settings.validate()?;
        let content = serde_json::to_string_pretty(settings)
            .map_err(|_| "云端备份设置序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("cloud-backup.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content).map_err(|_| "无法写入云端备份设置临时文件".to_string())?;
        if fs::rename(&temporary, self.cloud_backup_settings_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存云端备份设置".to_string());
        }
        Ok(())
    }

    /// The last successfully normalized public signal. An absent file is a
    /// normal first-run state; invalid data is left untouched rather than
    /// silently being mistaken for a current result.
    pub fn load_codex_reset_cache(&self) -> Result<Option<CodexResetStatus>, String> {
        let text = match fs::read_to_string(self.codex_reset_cache_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Codex 重置信号缓存不可读".to_string()),
        };
        let status: CodexResetStatus =
            serde_json::from_str(&text).map_err(|_| "Codex 重置信号缓存格式无效".to_string())?;
        status
            .validate_cached()
            .map_err(|_| "Codex 重置信号缓存格式无效".to_string())?;
        Ok(Some(status))
    }

    /// Replaces the prior public snapshot atomically after a successful
    /// signal read. It never stores the upstream raw payload or credentials.
    pub fn save_codex_reset_cache(&self, status: &CodexResetStatus) -> Result<(), String> {
        let content = serde_json::to_string_pretty(status)
            .map_err(|_| "Codex 重置信号缓存序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("codex-reset-cache.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content)
            .map_err(|_| "无法写入 Codex 重置信号缓存临时文件".to_string())?;
        if fs::rename(&temporary, self.codex_reset_cache_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存 Codex 重置信号缓存".to_string());
        }
        Ok(())
    }

    /// The persisted comparison baseline for after-the-fact Codex quota-reset
    /// detection. An absent file is a normal first-run state; invalid data is
    /// left untouched rather than silently mistaken for a current baseline.
    pub fn load_codex_quota_baseline(&self) -> Result<Option<CodexQuotaBaseline>, String> {
        let text = match fs::read_to_string(self.codex_quota_baseline_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Codex 官方额度基线不可读".to_string()),
        };
        let baseline: CodexQuotaBaseline =
            serde_json::from_str(&text).map_err(|_| "Codex 官方额度基线格式无效".to_string())?;
        baseline
            .validate()
            .map_err(|_| "Codex 官方额度基线格式无效".to_string())?;
        Ok(Some(baseline))
    }

    /// Replaces the prior baseline atomically after a successful official
    /// read. It never stores credentials or raw upstream payloads.
    pub fn save_codex_quota_baseline(&self, baseline: &CodexQuotaBaseline) -> Result<(), String> {
        let content = serde_json::to_string_pretty(baseline)
            .map_err(|_| "Codex 官方额度基线序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("codex-quota-baseline.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content)
            .map_err(|_| "无法写入 Codex 官方额度基线临时文件".to_string())?;
        if fs::rename(&temporary, self.codex_quota_baseline_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存 Codex 官方额度基线".to_string());
        }
        Ok(())
    }

    /// The last successful provider-usage snapshots for the custom tray panel. The
    /// file contains normalized readings plus query digests only: no API key,
    /// endpoint, raw response, or usage-script source is persisted here.
    pub(crate) fn load_usage_cache(&self) -> Result<Option<UsageCache>, String> {
        let text = match fs::read_to_string(self.usage_cache_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("托盘用量缓存不可读".to_string()),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| "托盘用量缓存格式无效".to_string())
    }

    pub(crate) fn save_usage_cache(&self, cache: &UsageCache) -> Result<(), String> {
        let content = serde_json::to_string_pretty(cache)
            .map_err(|_| "托盘用量缓存序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("usage-cache.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content).map_err(|_| "无法写入托盘用量缓存临时文件".to_string())?;
        if fs::rename(&temporary, self.usage_cache_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存托盘用量缓存".to_string());
        }
        Ok(())
    }

    pub(crate) fn clear_usage_cache(&self) -> Result<(), String> {
        match fs::remove_file(self.usage_cache_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清除托盘用量缓存".to_string()),
        }
    }

    /// The last successful local-session usage reports. It contains only
    /// normalized token totals and model labels, never a credential, endpoint,
    /// raw session record, or provider-quota reading.
    pub(crate) fn load_model_usage_cache(&self) -> Result<Option<ModelUsageCache>, String> {
        let text = match fs::read_to_string(self.model_usage_cache_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("本地会话快照不可读".to_string()),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| "本地会话快照格式无效".to_string())
    }

    /// Replaces every range snapshot atomically after a local-session scan.
    pub(crate) fn save_model_usage_cache(&self, cache: &ModelUsageCache) -> Result<(), String> {
        let content = serde_json::to_string_pretty(cache)
            .map_err(|_| "本地会话快照序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("model-usage-cache.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content).map_err(|_| "无法写入本地会话快照临时文件".to_string())?;
        if fs::rename(&temporary, self.model_usage_cache_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存本地会话快照".to_string());
        }
        Ok(())
    }

    /// The last successful local discovery scan, for display on the 发现 page
    /// before the next scan runs. The stored copy never carries credentials:
    /// import re-derives the draft from the live files. An absent file is a
    /// normal first-run state; invalid data is left untouched rather than
    /// silently mistaken for a current result.
    pub fn load_discovery_cache(&self) -> Result<Option<DiscoveryReport>, String> {
        let text = match fs::read_to_string(self.discovery_cache_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("发现扫描缓存不可读".to_string()),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|_| "发现扫描缓存格式无效".to_string())
    }

    pub fn save_discovery_cache(&self, report: &DiscoveryReport) -> Result<(), String> {
        let cached = report.cached_display();
        let content = serde_json::to_string_pretty(&cached)
            .map_err(|_| "发现扫描缓存序列化失败".to_string())?;
        fs::create_dir_all(&self.root).map_err(|_| "无法创建应用数据目录".to_string())?;
        let temporary = self
            .root
            .join(format!("discovery-cache.{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, content).map_err(|_| "无法写入发现扫描缓存临时文件".to_string())?;
        if fs::rename(&temporary, self.discovery_cache_path()).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("无法原子保存发现扫描缓存".to_string());
        }
        Ok(())
    }

    pub fn clear_discovery_cache(&self) -> Result<(), String> {
        match fs::remove_file(self.discovery_cache_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清除发现扫描缓存".to_string()),
        }
    }
}
