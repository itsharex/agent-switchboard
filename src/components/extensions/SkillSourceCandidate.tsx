import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, FileArchive, FolderGit2, LoaderCircle } from "lucide-react";
import { isBrowserDevelopment } from "../../lib/runtime";
import { Button } from "../Button";
import { Tooltip } from "../Tooltip";
import { CheckIcon, FolderOpenIcon } from "../icons";
import { skillCandidateHost, sourceErrorMessage, type SkillSourceKind, type SkillSourceRow } from "./skill-source-model";
import type { SkillSourceState } from "./useSkillSource";

export function SkillSourceLink({ url, label }: { url: string | null; label: string }) {
  const [error, setError] = useState<string | null>(null);
  if (!url) return null;
  return <>
    <Tooltip label={label}>
      <a href={url} target="_blank" rel="noreferrer" className="asb-btn asb-btn-icon" aria-label={label}
        onClick={(event) => {
          if (isBrowserDevelopment) return;
          event.preventDefault(); setError(null);
          void openUrl(url).catch((reason: unknown) => setError(sourceErrorMessage(reason, "无法打开来源链接")));
        }}><ExternalLink size={16} /></a>
    </Tooltip>
    {error && <span className="asb-warn-text" role="alert">{error}</span>}
  </>;
}

interface CandidateProps {
  row: SkillSourceRow;
  installed: boolean;
  imports: SkillSourceState["imports"];
  source: SkillSourceKind;
  busy: boolean;
  installs?: number;
}

export function SkillSourceCandidate({ row, installed, imports, source, busy, installs }: CandidateProps) {
  const { candidate } = row;
  const installing = imports.active === candidate.digest;
  const rejected = candidate.diagnostics.length > 0;
  const label = installed ? "已安装" : installing ? "正在安装" : rejected ? "不可安装" : "安装";
  return (
    <li className="asb-skill-source-card" aria-label={candidate.name}>
      <div className="asb-skill-source-card-heading">
        <h3 className="asb-group-title">{candidate.name}</h3>
        {installed && <span className="asb-skill-source-installed"><CheckIcon />已安装</span>}
        <SkillSourceLink url={row.sourceUrl} label={`查看 ${candidate.name} 来源`} />
      </div>
      <p className="asb-skill-source-origin" title={row.label}>
        {source === "catalog" || source === "directory" ? <FolderGit2 size={14} /> :
          source === "zip" ? <FileArchive size={14} /> : <FolderOpenIcon />}
        <span>{row.label}</span>
      </p>
      {candidate.description && <p className="asb-skill-source-description">{candidate.description}</p>}
      {rejected && <ul className="asb-skill-source-diagnostics" aria-label={`${candidate.name} 解析问题`}>
        {candidate.diagnostics.map((message, index) => <li key={index}>{sourceErrorMessage(message, "清单无效")}</li>)}
      </ul>}
      <div className="asb-skill-source-card-footer">
        <span className="asb-skill-source-meta">
          <span>{skillCandidateHost(candidate) ? "仅 Claude" : "Codex · Claude"}</span>
          <span className="asb-num">{candidate.fileCount} 个文件</span>
          {installs !== undefined && <span className="asb-num">{installs.toLocaleString()} 次目录安装</span>}
        </span>
        <Button variant={installed ? "secondary" : "primary"}
          disabled={busy || imports.busy || installed || rejected}
          onClick={() => void imports.install(candidate)} aria-label={`${label} ${candidate.name}`}>
          {installed ? <CheckIcon /> : installing ?
            <LoaderCircle size={16} className="asb-skill-source-spinner" /> : <Download size={16} />}
          {label}
        </Button>
      </div>
    </li>
  );
}
