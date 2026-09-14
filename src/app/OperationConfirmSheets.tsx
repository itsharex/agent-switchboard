import type { FilePreview } from "../api/client";
import { ConfirmSheet } from "../components/ConfirmSheet";
import { DiffView } from "../components/DiffView";
import { PreviewInspector } from "../components/PreviewInspector";
import { SwitchConfirmSheet } from "../components/SwitchConfirmSheet";
import { Time } from "../components/Time";
import { clientName } from "../lib/client-name";
import type { useProviders } from "./useProviders";
import type { useSwitchOperations } from "./useSwitchOperations";

interface OperationConfirmSheetsProps {
  /** Only an explicit activation supplies a candidate for write confirmation. */
  switchCandidate: { profileId: string; file: FilePreview } | null;
  busy: boolean;
  onCancelSwitch: () => void;
  operations: ReturnType<typeof useSwitchOperations>;
  providers: ReturnType<typeof useProviders>;
}

/** Every destructive or writing operation gets one explicit confirmation
 * sheet; nothing writes without it. */
export function OperationConfirmSheets({
  switchCandidate,
  busy,
  onCancelSwitch,
  operations,
  providers,
}: OperationConfirmSheetsProps) {
  const { undoPending, recoverLockPending } = operations;
  return (
    <>
      {switchCandidate && (
        <SwitchConfirmSheet
          filePreview={switchCandidate.file}
          busy={busy}
          onConfirm={() => void operations.runSwitch()}
          onCancel={onCancelSwitch}
        />
      )}
      {providers.pendingSave && (
        <ConfirmSheet
          title="确认保存并应用"
          details={[
            `将写入 ${providers.pendingSave.preview.preview.target}`,
            `变更 ${providers.pendingSave.preview.preview.changes.length} 个键`,
            ...providers.pendingSave.preview.preview.warnings.map((warning) => (
              <span key={warning} className="asb-warn-text">
                {warning}
              </span>
            )),
            <PreviewInspector
              filePreview={providers.pendingSave.preview}
              userConfigModel={null}
              userConfigWarnings={[]}
            />,
            `备份位置 ${providers.pendingSave.preview.preview.backupDir}`,
          ]}
          confirmLabel="确认保存并应用"
          onConfirm={() => void providers.runPendingSave()}
          onCancel={() => providers.setPendingSave(null)}
        />
      )}
      {providers.resetStorePending && (
        <ConfirmSheet
          title="清空旧供应商档案"
          details={[
            "将清空供应商档案（含运行参数）、客户端设置和切换记录。",
            "会保留备份、应用设置，以及 Codex / Claude Code 的实际配置和凭据。",
            "旧格式不会迁移；确认后请重新创建供应商档案。",
          ]}
          confirmLabel="清空并重新开始"
          destructive
          onConfirm={() => void providers.runResetStore()}
          onCancel={() => providers.setResetStorePending(false)}
        />
      )}
      {providers.deletePending && (
        <ConfirmSheet
          title="删除供应商"
          details={[
            `删除本地记录 ${providers.deletePending.kind === "generic"
              ? providers.deletePending.profile.name
              : providers.deletePending.record.profile.name}`,
            "不会修改当前客户端配置。",
          ]}
          confirmLabel="确认删除"
          destructive
          onConfirm={() => void providers.runDelete()}
          onCancel={() => providers.setDeletePending(null)}
        />
      )}
      {undoPending && (
        <ConfirmSheet
          title="撤回上一次切换"
          details={[
            `${clientName(undoPending.app)} ${
              undoPending.profileName ? `上次切换到「${undoPending.profileName}」` : "上次操作是恢复备份"
            }`,
            <>
              切换时间 <Time iso={undoPending.at} />
            </>,
            "将恢复该次切换前的备份；当前内容会先另行备份。",
            operations.undoDiff.state === "loading"
              ? "正在生成撤回后会写入的差异。"
              : operations.undoDiff.state === "error"
                ? <span className="asb-warn-text">{operations.undoDiff.message}</span>
                : operations.undoDiff.state === "ready" && operations.undoDiff.changes.length === 0
                  ? "当前受管配置已与将恢复的备份一致。"
                  : operations.undoDiff.state === "ready"
                    ? (
                        <DiffView
                          changes={operations.undoDiff.changes.map((change) => ({
                            ...change,
                            before: change.after,
                            after: change.before,
                          }))}
                          label="撤回后写入的差异"
                        />
                      )
                    : null,
          ]}
          confirmLabel="确认撤回"
          confirmDisabled={operations.undoDiff.state !== "ready"}
          onConfirm={() => void operations.runUndo()}
          onCancel={operations.cancelUndo}
        />
      )}
      {recoverLockPending && (
        <ConfirmSheet
          title="清理遗留锁"
          details={[
            `${clientName(recoverLockPending)} 的遗留写入锁将被删除。`,
            "仅在确认该客户端没有正在进行的切换时继续。",
          ]}
          confirmLabel="确认清理"
          destructive
          onConfirm={() => void operations.runRecoverStaleLock()}
          onCancel={() => operations.setRecoverLockPending(null)}
        />
      )}
    </>
  );
}
