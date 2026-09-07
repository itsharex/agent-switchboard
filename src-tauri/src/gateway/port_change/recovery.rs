//! Durable recovery for an interrupted gateway endpoint transaction.

use super::*;
use crate::local_state::LocalState;
use asb_switch::io::FsIo;
use asb_switch::{execute_rendered, RenderedWriteRequest};

pub(crate) fn recover_pending(
    local: &LocalState,
    state_path: &Path,
    state: &mut GatewayStateFile,
) -> Result<Option<BlockedPortChange>, String> {
    let journal = match read_journal(local) {
        Ok(Some(journal)) => journal,
        Ok(None) => return Ok(None),
        Err(reason) => {
            return Ok(Some(BlockedPortChange {
                from_port: state.port,
                to_port: state.port,
                apps: vec![],
                reason,
            }))
        }
    };
    match journal.stage {
        JournalStage::Prepared => match rollback_clients(local, &journal)
            .and_then(|_| rollback_state(state_path, state, &journal))
            .and_then(|_| cleanup_transaction(local, &journal))
        {
            Ok(()) => Ok(None),
            Err(reason) => Ok(Some(blocked(&journal, reason))),
        },
        JournalStage::Committed => match complete_committed_state(state_path, state, &journal) {
            Ok(()) => match cleanup_transaction(local, &journal) {
                Ok(()) => Ok(None),
                Err(error) => Ok(Some(blocked(
                    &journal,
                    format!("端口修改已完成，但无法清理恢复材料：{error}"),
                ))),
            },
            Err(reason) => Ok(Some(blocked(&journal, reason))),
        },
    }
}

/// Runtime rollback after a failure before the journal commit marker. It
/// restores the exact client snapshots, removes only its own latest audit
/// facts, restores the saved port, then clears only this transaction's files.
pub(super) fn rollback_runtime(
    controller: &GatewayController,
    local: &LocalState,
    journal: &PortChangeJournal,
) -> Result<(), String> {
    rollback_clients(local, journal)?;
    let current_port = controller.configured_port();
    if current_port == journal.to_port {
        controller.persist_port(journal.from_port)?;
    } else if current_port != journal.from_port {
        return Err("网关端口在事务外被修改，拒绝覆盖".to_string());
    }
    cleanup_transaction(local, journal)
}

pub(super) fn cleanup_transaction(
    local: &LocalState,
    journal: &PortChangeJournal,
) -> Result<(), String> {
    let directory = backup_dir(local, &journal.id)?;
    match fs::remove_dir_all(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("无法清除端口修改备份".to_string()),
    }
    match fs::remove_file(journal_path(local)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("无法清除端口修改恢复记录".to_string()),
    }
}

pub(crate) fn discard_blocked(
    controller: &GatewayController,
    local: &LocalState,
) -> Result<(), String> {
    if controller.blocked_recovery().is_none() {
        return Err("当前没有待处理的端口修改恢复".to_string());
    }
    controller.synchronize_port_to_listener()?;
    match read_journal(local) {
        Ok(Some(journal)) => cleanup_transaction(local, &journal)?,
        Ok(None) => {}
        Err(_) => match fs::remove_file(journal_path(local)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法清除无效的端口修改恢复记录".to_string()),
        },
    }
    controller.clear_blocked_port_change()?;
    Ok(())
}

fn rollback_clients(local: &LocalState, journal: &PortChangeJournal) -> Result<(), String> {
    let directory = backup_dir(local, &journal.id)?;
    for client in journal.clients.iter().rev() {
        let target = local
            .target(client.app)
            .map_err(|error| format!("无法解析客户端配置路径：{error}"))?;
        let live = fs::read(&target).map_err(|_| "客户端配置文件不可读，拒绝回滚".to_string())?;
        let live_hash = hash_bytes(&live);
        if live_hash == client.before_hash {
            continue;
        }
        if live_hash != client.after_hash {
            if client.history.is_some() {
                return Err("客户端配置在事务外被修改，拒绝覆盖".to_string());
            }
            // This client never reached the executor callback, so a
            // different live document is an external edit before it was
            // touched. Preserve it and continue rolling back earlier files.
            continue;
        }
        let backup = fs::read_to_string(directory.join(&client.backup_file))
            .map_err(|_| "端口修改备份不可读，无法回滚".to_string())?;
        if hash_bytes(backup.as_bytes()) != client.before_hash {
            return Err("端口修改备份与记录哈希不符，拒绝回滚".to_string());
        }
        let backup_dir = local.backup_dir();
        let outcome = execute_rendered(
            &FsIo,
            &RenderedWriteRequest {
                target: &target,
                app: client.app,
                backup_dir: &backup_dir,
                expected_hash: &client.after_hash,
                expected_target_existed: true,
                rendered: &backup,
                reason: "gateway-port-rollback",
            },
            |_| Ok(()),
        )
        .map_err(|error| format!("通过配置执行器回滚客户端配置失败：{error}"))?;
        for warning in outcome.warnings {
            log::warn!("网关端口修改回滚完成，但配置执行器有警告：{warning}");
        }
    }
    for client in journal.clients.iter().rev() {
        if let Some(record) = &client.history {
            local.configuration().remove_config_write_if_last(record)?;
        }
    }
    Ok(())
}

fn rollback_state(
    state_path: &Path,
    state: &mut GatewayStateFile,
    journal: &PortChangeJournal,
) -> Result<(), String> {
    if state.port == journal.from_port {
        return Ok(());
    }
    if state.port != journal.to_port {
        return Err("网关端口在事务外被修改，拒绝覆盖".to_string());
    }
    let mut restored = state.clone();
    restored.port = journal.from_port;
    write_state(state_path, &restored)?;
    *state = restored;
    Ok(())
}

fn complete_committed_state(
    state_path: &Path,
    state: &mut GatewayStateFile,
    journal: &PortChangeJournal,
) -> Result<(), String> {
    if state.port == journal.to_port {
        return Ok(());
    }
    if state.port != journal.from_port {
        return Err("网关端口在事务外被修改，拒绝覆盖".to_string());
    }
    let mut completed = state.clone();
    completed.port = journal.to_port;
    write_state(state_path, &completed)?;
    *state = completed;
    Ok(())
}
