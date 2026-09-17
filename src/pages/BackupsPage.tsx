import { useState } from "react";
import type { BackupRecord, ConfigWriteRecord } from "../api/client";
import type { useCloudBackup } from "../app/useCloudBackup";
import { BackupHistory } from "../components/BackupHistory";
import { Button } from "../components/Button";
import { CloudBackupPanel } from "../components/CloudBackupPanel";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { Tabs } from "../components/Tabs";
import { Time } from "../components/Time";
import { clientName } from "../lib/client-name";

interface BackupsPageProps {
  records: BackupRecord[];
  busy: boolean;
  lastSwitch: ConfigWriteRecord | null;
  cloudBackup: ReturnType<typeof useCloudBackup>;
  onRestore: (backupId: string) => void;
  onUndo: (lastSwitch: ConfigWriteRecord) => void;
  onOpenDir: () => void;
}

type BackupTab = "local" | "cloud";

/** Backup history with restore, undo of the last switch, and the handoff to
 * the system file manager for cleanup. */
export function BackupsPage({
  records,
  busy,
  lastSwitch,
  cloudBackup,
  onRestore,
  onUndo,
  onOpenDir,
}: BackupsPageProps) {
  const [activeTab, setActiveTab] = useState<BackupTab>("local");

  return (
    <section className="asb-panel" aria-label="备份">
      {/* 设置内容区的子页：模块级 h3 标题独占第一行，备份类型页签在第二行。 */}
      <ModuleHeader
        title="备份"
        primary={
          <Tabs value={activeTab} onChange={setActiveTab} scope="backup" label="备份类型"
            tabs={[{ value: "local", label: "本地备份", controls: "backup-local-panel" },
              { value: "cloud", label: "加密云端备份", controls: "backup-cloud-panel" }]} />
        }
      />
      {/* Both tabpanels stay mounted so each tab's aria-controls always
          resolves; inactive content unmounts inside its hidden panel. */}
      <div
        id="backup-local-panel"
        className="asb-backup-local"
        role="tabpanel"
        aria-labelledby="backup-local-tab"
        hidden={activeTab !== "local"}
      >
        {activeTab === "local" && (
          <>
            <div className="asb-backup-toolbar">
              <div className="asb-panel-actions">
                {lastSwitch && lastSwitch.operation !== "gatewayPortChange" && (
                  <Button
                    variant="danger"
                    disabled={busy}
                    onClick={() => onUndo(lastSwitch)}
                  >
                    {lastSwitch.profileName ? "撤回上一次切换" : "撤回上一次配置写入"}
                  </Button>
                )}
                <Button variant="secondary" onClick={onOpenDir}>
                  打开备份文件夹
                </Button>
              </div>
            </div>
            {lastSwitch && (
              <p className="asb-scope-note">
                上次操作：{clientName(lastSwitch.app)}
                {lastSwitch.operation === "projection" && lastSwitch.profileName
                  ? ` 已投影供应商「${lastSwitch.profileName}」`
                  : lastSwitch.operation === "projection"
                    ? " 已写入客户端通用配置"
                    : lastSwitch.operation === "restore"
                      ? " 恢复了备份"
                      : " 已修改网关监听端口"}
                ，<Time iso={lastSwitch.at} />。
              </p>
            )}
            <BackupHistory records={records} busy={busy} onRestore={onRestore} />
          </>
        )}
      </div>
      <div
        id="backup-cloud-panel"
        role="tabpanel"
        aria-labelledby="backup-cloud-tab"
        hidden={activeTab !== "cloud"}
      >
        {activeTab === "cloud" && (
          <CloudBackupPanel
            settings={cloudBackup.settings}
            loaded={cloudBackup.loaded}
            busy={busy}
            onSave={cloudBackup.saveSettings}
            onTestConnection={cloudBackup.testConnection}
            onUpload={cloudBackup.upload}
            onRestore={cloudBackup.restore}
          />
        )}
      </div>
    </section>
  );
}
