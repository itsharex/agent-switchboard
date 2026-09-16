import { useEffect, useState } from "react";
import * as api from "../../api/codex-history";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Select } from "../Select";
import { Table } from "../Table";
import type { CodexOperations } from "./operations";

type Mode = "legacy" | "manual";

const MODES = [
  { value: "legacy", label: "已知遗留桶（默认）" },
  { value: "manual", label: "手动选择桶" },
] as const;

function bucketName(providerId: string): string {
  return providerId === api.UNIFIED_PROVIDER_ID ? `${providerId}（统一桶）` : providerId;
}

/**
 * 跨供应商统一历史（S03）。遗留来源桶（CC 预设 id、旧工具自定义 id）的存量
 * 会话会被并入统一桶；扫描只读，迁移与还原都由后端逐对象备份后写入。
 * 这里只发起类型化调用并展示结果，从不自行改写会话文件。
 */
export function HistoryUnifyPane({ operations }: { operations: CodexOperations }) {
  const { run, busy, changed } = operations;
  const [view, setView] = useState<api.CodexHistoryBuckets | null>(null);
  const [mode, setMode] = useState<Mode>("legacy");
  const [selected, setSelected] = useState<string[]>([]);
  const [confirming, setConfirming] = useState<"migrate" | "restore" | null>(null);
  const [results, setResults] = useState<string[]>([]);
  const [skips, setSkips] = useState<string[]>([]);

  const load = async () => setView(await api.scanCodexHistoryBuckets());
  useEffect(() => { void run(async () => setView(await api.scanCodexHistoryBuckets())); }, [run]);
  if (!view) return <p role="status">正在读取 Codex 历史桶…</p>;

  const migratable = view.buckets.filter((bucket) => bucket.providerId !== api.UNIFIED_PROVIDER_ID);
  const useLegacy = mode === "legacy";
  const canMigrate = useLegacy ? migratable.length > 0 : selected.length > 0;

  const reset = () => { setConfirming(null); setResults([]); setSkips([]); };
  const settle = (skippedReason: string | null, line: string) => {
    const skip = api.describeCodexHistorySkip(skippedReason);
    setSkips(skip ? [skip] : []);
    setResults(skip ? [] : [line]);
  };

  const migrate = () => void run(async () => {
    const outcome = await api.migrateCodexHistoryToUnified(useLegacy ? null : selected);
    settle(
      outcome.skippedReason,
      `已把${outcome.legacyDefault ? "已知遗留桶" : `所选 ${outcome.sourceProviderIds.length} 个桶`}迁入统一桶：改写 ${outcome.migratedJsonlFiles} 个会话文件、${outcome.migratedStateRows} 条 state DB 记录。原对象已入备份账本。`,
    );
    setConfirming(null);
    await load();
    changed(outcome.skippedReason ? "历史迁移未执行，详见下方说明。" : "Codex 历史已迁入统一桶；原对象已备份。");
  });

  const restore = () => void run(async () => {
    const outcome = await api.restoreCodexHistoryFromBackups();
    settle(
      outcome.skippedReason,
      `已按备份账本还原：${outcome.restoredJsonlFiles} 个会话文件、${outcome.restoredStateRows} 条 state DB 记录；迁移之后新增的会话不受影响。`,
    );
    setConfirming(null);
    await load();
    changed(outcome.skippedReason ? "历史还原未执行，详见下方说明。" : "Codex 历史已按备份账本还原。");
  });

  const toggle = (providerId: string) =>
    setSelected((current) =>
      current.includes(providerId)
        ? current.filter((entry) => entry !== providerId)
        : [...current, providerId]);

  return <div className="asb-provider-section-fields">
    <p className="asb-scope-note">统一历史把遗留来源桶（CC 预设 id、旧工具写入的自定义 id）的存量会话并入统一桶，让不同供应商的会话在 Codex 里连成一条时间线。扫描只读；迁移与还原都会先逐对象备份，失败不写完成标记、可安全重跑。</p>
    <p className="asb-scope-note">状态：{view.migrationCompleted ? "当前目录已完成过迁移" : "当前目录尚未迁移"} · 备份账本 {view.backupAvailable ? "可用" : "不可用"}</p>

    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => { reset(); await load(); })}>重新扫描 Codex 历史桶</Button>
    </div>

    {view.buckets.length === 0
      ? <p role="status">没有发现任何会话桶。</p>
      : <Table ariaLabel="Codex 历史桶列表" rows={view.buckets} rowKey={(bucket) => bucket.providerId} columns={[
          ...(useLegacy ? [] : [{
            key: "pick", header: "迁移",
            render: (bucket: api.CodexHistoryBucket) => bucket.providerId === api.UNIFIED_PROVIDER_ID
              ? <span className="asb-scope-note">统一桶本身</span>
              : <Checkbox label={`迁移 ${bucket.providerId}`} ariaLabel={`迁移 ${bucket.providerId}`}
                  checked={selected.includes(bucket.providerId)} disabled={busy}
                  onChange={() => toggle(bucket.providerId)} />,
          }]),
          { key: "bucket", header: "来源桶", render: (bucket: api.CodexHistoryBucket) => bucketName(bucket.providerId) },
          { key: "jsonl", header: "会话文件", render: (bucket: api.CodexHistoryBucket) => String(bucket.jsonlFiles) },
          { key: "state", header: "State DB 记录", render: (bucket: api.CodexHistoryBucket) => String(bucket.stateRows) },
        ]} />}

    <label className="asb-field"><span>迁移来源</span>
      <Select ariaLabel="Codex 历史迁移来源模式" value={mode} disabled={busy}
        onChange={(value) => { setMode(value as Mode); reset(); }}
        options={MODES} />
    </label>

    <div className="asb-form-actions">
      <Button variant="primary" disabled={busy || !canMigrate} onClick={() => { reset(); setConfirming("migrate"); }}>迁移到统一桶</Button>
      <Button variant="secondary" disabled={busy || !view.backupAvailable} onClick={() => { reset(); setConfirming("restore"); }}>按备份账本还原</Button>
    </div>

    {confirming === "migrate" && <div role="group" aria-label="确认迁移 Codex 历史">
      <p className="asb-warn-text">迁移会改写 {useLegacy ? "已知遗留桶" : `所选 ${selected.length} 个桶`} 里会话的来源标注，并把原对象备份到账本。请确认当前 Codex 已关闭再继续。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setConfirming(null)}>取消迁移</Button>
      <Button variant="primary" disabled={busy} onClick={migrate}>确认迁移 Codex 历史</Button>
    </div>}

    {confirming === "restore" && <div role="group" aria-label="确认还原 Codex 历史">
      <p className="asb-warn-text">还原只翻回账本内的对象，迁移之后新增的会话不会被改动。请确认当前 Codex 已关闭再继续。</p>
      <Button variant="secondary" disabled={busy} onClick={() => setConfirming(null)}>取消还原</Button>
      <Button variant="primary" disabled={busy} onClick={restore}>确认还原 Codex 历史</Button>
    </div>}

    {results.length > 0 && <section aria-label="Codex 历史迁移结果">
      {results.map((line) => <p key={line} className="asb-scope-note">{line}</p>)}
    </section>}
    {skips.map((line) => <p key={line} className="asb-warn-text">{line}</p>)}
  </div>;
}
