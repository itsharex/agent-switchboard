import { useEffect, useRef, useState } from "react";
import type { ExtensionListItem, SkillCandidateDto } from "../../api/client";
import { skillCandidateHost, skillCandidateInstalled, sourceErrorMessage, type SkillSourceActions } from "./skill-source-model";

export function useSkillSourceInstall(actions: SkillSourceActions, items: ExtensionListItem[], busy: boolean) {
  const lock = useRef<string | null>(null);
  const latest = useRef({ actions, items, busy });
  latest.current = { actions, items, busy };
  const mounted = useRef(true);
  const [active, setActive] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const install = async (candidate: SkillCandidateDto) => {
    const current = latest.current;
    if (!mounted.current || current.busy || lock.current || candidate.diagnostics.length > 0 ||
      skillCandidateInstalled(candidate, current.items)) return;
    lock.current = candidate.digest;
    setActive(candidate.digest);
    setError(null);
    try {
      const saved = await current.actions.onImport(candidate.digest, candidate.name, skillCandidateHost(candidate));
      if (!mounted.current) return;
      if (!saved) setError(`${candidate.name} 安装未完成；详细原因见操作通知`);
    } catch (reason) {
      lock.current = null;
      if (mounted.current) setError(sourceErrorMessage(reason, `${candidate.name} 安装失败，请重试`));
    } finally {
      lock.current = null;
      if (mounted.current) setActive(null);
    }
  };
  return { install, active, busy: active !== null, isLocked: () => lock.current !== null, error };
}
