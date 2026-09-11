import { useState } from "react";
import type { AppKind, ExtensionMutation, SkillCandidateDto } from "../../api/client";

export interface SkillSourceActions {
  onScanLocal: (root: string) => Promise<SkillCandidateDto[] | null>;
  onResolveGithub: (
    repository: string,
    subpath: string,
    ref: string | null,
  ) => Promise<SkillCandidateDto[] | null>;
  onImport: (digest: string, name: string, host: AppKind | null) => Promise<ExtensionMutation | null>;
  onPickDirectory: () => Promise<string | null>;
}

export function useSkillSource(actions: SkillSourceActions) {
  const [source, setSource] = useState<"github" | "local">("github");
  const [fields, setFields] = useState({ root: "", repository: "", subpath: "", ref: "" });
  const [host, setHost] = useState<"all" | AppKind>("all");
  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<SkillCandidateDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const changeSource = (next: "github" | "local") => {
    setSource(next);
    setCandidates(null);
    setError(null);
    setQuery("");
  };
  const changeField = (key: keyof typeof fields, value: string) => {
    setFields((previous) => ({ ...previous, [key]: value }));
    setCandidates(null);
    setError(null);
  };
  const search = async (rootOverride?: string) => {
    setError(null);
    const root = rootOverride ?? fields.root;
    if (source === "local" ? !root.trim() : !fields.repository.trim() || !fields.subpath.trim()) {
      setError(source === "local" ? "请输入本地来源目录" : "请填写 GitHub 仓库和 Skill 子目录");
      return;
    }
    setLoading(true);
    try {
      const result =
        source === "local"
          ? await actions.onScanLocal(root.trim())
          : await actions.onResolveGithub(
              fields.repository.trim(),
              fields.subpath.trim(),
              fields.ref.trim() || null,
            );
      if (result !== null) setCandidates(result);
      else setError(candidates === null ? "来源读取失败，请检查地址后重试" : "来源读取失败，仍显示上次结果");
    } finally {
      setLoading(false);
    }
  };
  const pickDirectory = async () => {
    setError(null);
    let picked: string | null;
    try {
      picked = (await actions.onPickDirectory())?.trim() ?? null;
    } catch {
      setError("无法打开目录选择对话框");
      return;
    }
    if (!picked) return;
    setFields((previous) => ({ ...previous, root: picked }));
    setCandidates(null);
    setQuery("");
    await search(picked);
  };
  const visible =
    candidates?.filter((candidate) =>
      `${candidate.name} ${candidate.description ?? ""}`.toLowerCase().includes(query.trim().toLowerCase()),
    ) ?? [];
  return {
    source,
    fields,
    host,
    setHost,
    query,
    setQuery,
    candidates,
    visible,
    error,
    loading,
    changeSource,
    changeField,
    search,
    pickDirectory,
  };
}

export type SkillSourceState = ReturnType<typeof useSkillSource>;
