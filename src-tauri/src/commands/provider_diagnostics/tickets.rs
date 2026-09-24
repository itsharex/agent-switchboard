use super::{failed, snapshot::Snapshot};
use crate::commands::error::CommandError;
use asb_core::AppKind;
use asb_switch::FilePreview;
use std::{collections::HashMap, sync::{Arc, Mutex}, time::{Duration, Instant}};

#[derive(Clone)]
pub(super) struct CompletedRepair {
    pub app: AppKind,
    pub profile_id: String,
    pub backup_id: String,
    pub after: Snapshot,
}

pub(super) enum Ticket {
    Repair { app: AppKind, profile_id: String, before: Snapshot, file: FilePreview },
    Completed(CompletedRepair),
    Undo { completed: CompletedRepair, fingerprint: String },
}

#[derive(Clone, Default)]
pub(crate) struct ProviderRepairPreparations(Arc<Mutex<HashMap<String, (Instant, Ticket)>>>);

impl ProviderRepairPreparations {
    pub(super) fn issue(&self, ticket: Ticket) -> Result<String, CommandError> {
        let mut entries = self.0.lock().map_err(|_| unavailable())?;
        entries.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(30 * 60));
        if entries.len() >= 64 {
            return Err(failed("providerDiagnostics.backend.tooManyPreviews", "打开的诊断预览过多，请关闭旧预览后重试"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        entries.insert(id.clone(), (Instant::now(), ticket));
        Ok(id)
    }

    pub(super) fn take(&self, id: &str) -> Result<Ticket, CommandError> {
        self.0.lock().map_err(|_| unavailable())?.remove(id)
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(30 * 60))
            .map(|(_, ticket)| ticket).ok_or_else(unavailable)
    }

    pub(super) fn completed(&self, id: &str) -> Result<CompletedRepair, CommandError> {
        let entries = self.0.lock().map_err(|_| unavailable())?;
        match entries.get(id) {
            Some((at, Ticket::Completed(completed))) if at.elapsed() < Duration::from_secs(30 * 60) => Ok(completed.clone()),
            _ => Err(unavailable()),
        }
    }

    pub(super) fn cancel(&self, id: &str) -> Result<(), CommandError> {
        let mut entries = self.0.lock().map_err(|_| unavailable())?;
        if entries.get(id).is_some_and(|(_, ticket)| !matches!(ticket, Ticket::Completed(_))) {
            entries.remove(id);
        }
        Ok(())
    }
}

pub(super) fn unavailable() -> CommandError {
    failed("providerDiagnostics.backend.previewExpired", "诊断预览已取消、过期或使用，请重新预览；已完成的修复仍有持久备份")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_or_consumed_tickets_cannot_be_reused() {
        let tickets = ProviderRepairPreparations::default();
        let make = || Ticket::Undo { completed: CompletedRepair {
            app: AppKind::Codex, profile_id: "profile".into(), backup_id: "backup".into(),
            after: super::super::snapshot::fixture(),
        }, fingerprint: "candidate".into() };
        let cancelled = tickets.issue(make()).unwrap();
        tickets.cancel(&cancelled).unwrap();
        assert!(tickets.take(&cancelled).is_err());
        let consumed = tickets.issue(make()).unwrap();
        assert!(matches!(tickets.take(&consumed).unwrap(), Ticket::Undo { .. }));
        assert!(tickets.take(&consumed).is_err());
        let expired = tickets.issue(make()).unwrap();
        tickets.0.lock().unwrap().get_mut(&expired).unwrap().0 = Instant::now() - Duration::from_secs(31 * 60);
        assert!(tickets.take(&expired).is_err());
    }
}
