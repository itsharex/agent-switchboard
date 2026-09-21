import type { Ref } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "./Button";
import { ConnectivityIcon, EditIcon, TrashIcon, UsageIcon } from "./icons";
import { Tooltip } from "./Tooltip";

export function ProviderEndpoint({ url, link = false }: { url: string; link?: boolean }) {
  let host = url;
  try { host = new URL(url).host; } catch { /* Keep the supplied endpoint readable. */ }
  if (!link) return <span title={url}>{host}</span>;
  return <a className="asb-row-host" href={url} title={url}
    onClick={(event) => { event.preventDefault(); void openUrl(url); }}>{host}</a>;
}

export function ProviderActivateButton({ name, onActivate }: { name: string; onActivate: () => void }) {
  return <Tooltip label={`启用 ${name}`}>
    <Button variant="primary" className="asb-row-activate" aria-label={`启用 ${name}`} onClick={onActivate}>启用</Button>
  </Tooltip>;
}

export function ProviderLoginButton({ name, open, onToggle }: { name: string; open: boolean; onToggle: () => void }) {
  const label = open ? `收起 ${name} 登录` : `重新登录 ${name}`;
  return <Tooltip label={label}>
    <Button variant="secondary" className={`asb-row-activate${open ? " is-active" : ""}`}
      aria-label={label} aria-expanded={open} onClick={onToggle}>{open ? "收起登录" : "重新登录"}</Button>
  </Tooltip>;
}

interface Props {
  name: string;
  onEdit?: () => void;
  onDelete?: () => void;
  test?: { id: string; open: boolean; onToggle: () => void; trigger: Ref<HTMLButtonElement> };
  usage?: { id: string; configured: boolean; open: boolean; onOpen: () => void };
}

/** Both clients use the same actions and disclosure semantics. */
export function ProviderRowActions({ name, onEdit, onDelete, test, usage }: Props) {
  const testLabel = test?.open ? `收起 ${name} 供应商测试` : `测试 ${name} 供应商`;
  const usageLabel = !usage?.configured ? `配置 ${name} 用量` : usage.open ? `收起 ${name} 用量详情` : `查看 ${name} 用量详情`;
  return <>
    {onEdit && <Tooltip label={`编辑 ${name}`}>
      <Button variant="icon" aria-label={`编辑 ${name}`} onClick={onEdit}><EditIcon /></Button>
    </Tooltip>}
    {test && <Tooltip label={testLabel}>
      <Button ref={test.trigger} variant="icon" className={test.open ? "is-active" : undefined}
        aria-label={testLabel} aria-controls={test.open ? test.id : undefined} aria-expanded={test.open}
        onClick={test.onToggle}><ConnectivityIcon /></Button>
    </Tooltip>}
    {usage && <Tooltip label={usageLabel}>
      <Button variant="icon" className={usage.configured && usage.open ? "is-active" : undefined}
        aria-label={usageLabel} aria-controls={usage.configured && usage.open ? usage.id : undefined}
        aria-expanded={usage.configured ? usage.open : undefined} onClick={usage.onOpen}><UsageIcon /></Button>
    </Tooltip>}
    {onDelete && <Tooltip label={`删除 ${name}`}>
      <Button variant="icon" aria-label={`删除 ${name}`} onClick={onDelete}><TrashIcon /></Button>
    </Tooltip>}
  </>;
}
