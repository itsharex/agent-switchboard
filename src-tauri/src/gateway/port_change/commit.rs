//! Confirmed endpoint transaction and listener handoff.

use super::*;
use crate::local_state::LocalState;
use asb_core::contracts::{ConfigWriteRecord, WriteOperation};
use asb_switch::io::FsIo;
use asb_switch::{execute_rendered, RenderedWriteRequest};
use std::path::PathBuf;
use uuid::Uuid;

pub(crate) fn commit(
    controller: &GatewayController,
    local: &LocalState,
    preparations: &PortChangePreparations,
    preparation_id: &str,
) -> Result<GatewayPortChangeResult, String> {
    let _commit_lock = preparations.lock_commit()?;
    let prepared = preparations.take(preparation_id)?;
    if !controller.state_available() {
        return Err("本机协议网关状态不可用，请先恢复状态文件后再修改端口".to_string());
    }
    if controller.blocked_recovery().is_some() || journal_path(local).exists() {
        return Err("存在未完成的端口修改恢复，请先处理后再试".to_string());
    }
    if controller.configured_port() != prepared.plan.from_port {
        return Err("网关端口已变更，本次预览已失效，请重新发起修改".to_string());
    }
    let observed = prepare::observe_clients(
        controller,
        local,
        prepared.plan.from_port,
        prepared.plan.to_port,
    )?;
    let snapshots: Vec<_> = observed
        .iter()
        .map(|entry| entry.snapshot.clone())
        .collect();
    if snapshots != prepared.snapshots {
        return Err("客户端配置、登录缓存或供应商路由已在预览后变化，请重新发起修改".to_string());
    }
    let builds = endpoint_builds(local, observed)?;
    if !controller.enter_maintenance_and_drain() {
        controller.exit_maintenance();
        return Err("存在长时间未结束的网关请求，本次修改已取消，服务未中断".to_string());
    }
    if let Err(error) = controller.spawn_serve(prepared.listener.clone()) {
        prepared.listener.stop();
        controller.exit_maintenance();
        return Err(error);
    }
    let result = apply_changes(controller, local, &prepared, &builds);
    if result.is_err() {
        prepared.listener.stop();
        controller.exit_maintenance();
    }
    result
}

struct EndpointBuild {
    client: GatewayPortChangeClient,
    target: PathBuf,
    before_hash: String,
    rendered: String,
}

fn endpoint_builds(
    local: &LocalState,
    observed: Vec<ObservedClient>,
) -> Result<Vec<EndpointBuild>, String> {
    observed
        .into_iter()
        .filter_map(|entry| entry.client.clone().map(|client| (client, entry)))
        .map(|(client, entry)| {
            let current = entry.configuration.ok_or_else(|| {
                "网关所属客户端配置缺失，本次预览已失效；请重新发起修改".to_string()
            })?;
            let target = local
                .target(client.app)
                .map_err(|error| format!("无法解析客户端配置路径：{error}"))?;
            let rendered =
                adapter::render_gateway_base_url(client.app, &current, &client.new_base_url)
                    .map_err(|_| "无法生成仅修改端口的客户端配置，本次修改已取消".to_string())?;
            Ok(EndpointBuild {
                client,
                target,
                before_hash: entry.snapshot.config.hash,
                rendered,
            })
        })
        .collect()
}

fn apply_changes(
    controller: &GatewayController,
    local: &LocalState,
    prepared: &PreparedPortChange,
    builds: &[EndpointBuild],
) -> Result<GatewayPortChangeResult, String> {
    let mut journal = create_journal(local, &prepared.plan, builds)?;
    let mut warnings = Vec::new();
    for (index, build) in builds.iter().enumerate() {
        let backup_dir = local.backup_dir();
        let store = local.configuration();
        let execution = execute_rendered(
            &FsIo,
            &RenderedWriteRequest {
                target: &build.target,
                app: build.client.app,
                backup_dir: &backup_dir,
                expected_hash: &build.before_hash,
                expected_target_existed: true,
                rendered: &build.rendered,
                reason: "gateway-port-change",
            },
            |outcome| {
                let record = ConfigWriteRecord {
                    app: build.client.app,
                    profile_id: Some(build.client.profile_id.clone()),
                    profile_name: Some(build.client.profile_name.clone()),
                    content_hash: outcome.final_hash.clone(),
                    backup_id: outcome.backup.id.clone(),
                    at: outcome.backup.created_at.clone(),
                    operation: WriteOperation::GatewayPortChange,
                };
                journal.clients[index].history = Some(record.clone());
                write_journal(local, &journal)?;
                store.record_config_write(record)
            },
        );
        let outcome = match execution {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(fail_precommit(
                    controller,
                    local,
                    &journal,
                    format!("端口修改写入客户端配置失败：{error}"),
                ));
            }
        };
        warnings.extend(outcome.warnings);
    }
    if let Err(error) = controller.persist_port(prepared.plan.to_port) {
        return Err(fail_precommit(controller, local, &journal, error));
    }
    journal.stage = JournalStage::Committed;
    if let Err(error) = write_journal(local, &journal) {
        return Err(fail_precommit(controller, local, &journal, error));
    }
    if let Some(old_listener) = controller.replace_listener(prepared.listener.clone()) {
        old_listener.stop();
    }
    controller.exit_maintenance();
    if let Err(error) = recovery::cleanup_transaction(local, &journal) {
        let warning = format!("监听端口已修改，但无法清理恢复材料：{error}");
        controller.block_port_change(blocked(&journal, warning.clone()));
        log::warn!("{warning}");
        warnings.push(warning);
    }
    Ok(GatewayPortChangeResult {
        from_port: prepared.plan.from_port,
        to_port: prepared.plan.to_port,
        clients: prepared.plan.clients.clone(),
        warnings,
    })
}

fn create_journal(
    local: &LocalState,
    plan: &GatewayPortChangePlan,
    builds: &[EndpointBuild],
) -> Result<PortChangeJournal, String> {
    let id = Uuid::new_v4().to_string();
    let directory = backup_dir(local, &id)?;
    let result = (|| {
        fs::create_dir_all(&directory).map_err(|_| "无法创建端口修改备份目录".to_string())?;
        let mut clients = Vec::with_capacity(builds.len());
        for build in builds {
            let live = fs::read(&build.target)
                .map_err(|_| "客户端配置已不可读，本次预览已失效；请重新发起修改".to_string())?;
            if hash_bytes(&live) != build.before_hash {
                return Err("客户端配置在预览后被修改，本次预览已失效；请重新发起修改".to_string());
            }
            let backup_file = backup_file_for(build.client.app).to_string();
            let backup_path = directory.join(&backup_file);
            fs::write(&backup_path, &live).map_err(|_| "无法备份客户端配置".to_string())?;
            let verified =
                fs::read(&backup_path).map_err(|_| "无法校验客户端配置备份".to_string())?;
            if verified != live || hash_bytes(&verified) != build.before_hash {
                return Err("客户端配置备份校验失败".to_string());
            }
            clients.push(JournalClient {
                app: build.client.app,
                before_hash: build.before_hash.clone(),
                after_hash: hash_bytes(build.rendered.as_bytes()),
                backup_file,
                history: None,
            });
        }
        let journal = PortChangeJournal {
            version: JOURNAL_VERSION,
            id,
            from_port: plan.from_port,
            to_port: plan.to_port,
            stage: JournalStage::Prepared,
            clients,
        };
        write_journal(local, &journal)?;
        Ok(journal)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

fn fail_precommit(
    controller: &GatewayController,
    local: &LocalState,
    journal: &PortChangeJournal,
    primary: String,
) -> String {
    match recovery::rollback_runtime(controller, local, journal) {
        Ok(()) => format!("{primary}；已回滚本次端口修改"),
        Err(recovery) => {
            controller.block_port_change(blocked(journal, recovery.clone()));
            format!("{primary}；回滚未完成：{recovery}；已保留恢复记录")
        }
    }
}
