import { uiMessage } from "../../i18n/errors";
import { useState } from "react";
import { scanSkillZip } from "../../api/extensions/skill-sources";
import type { SkillSourceActions, SkillSourceResult } from "./skill-source-model";
import { useSkillSourceRequests } from "./useSkillSourceRequests";

type FileSource = "local" | "zip";

export function useSkillSourceFiles(actions: SkillSourceActions & { busy: boolean }) {
  const [paths, setPaths] = useState({ local: "", zip: "" });
  const requests = useSkillSourceRequests<SkillSourceResult>(actions.busy);
  const changePath = (kind: FileSource, value: string) => {
    setPaths((previous) => ({ ...previous, [kind]: value }));
    requests.clear();
  };
  const read = async (kind: FileSource, path: string): Promise<SkillSourceResult> => {
    const candidates = kind === "local" ? await actions.onScanLocal(path) : await scanSkillZip(path);
    if (candidates === null) throw uiMessage("extensions.sources.readFailed");
    return { kind, candidates, label: path };
  };
  const scan = async (kind: FileSource) => {
    const path = paths[kind].trim();
    if (!path) {
      requests.setError(kind === "zip" ? uiMessage("extensions.sources.enterZipPath") : uiMessage("extensions.sources.enterLocalPath"));
      return;
    }
    await requests.run(() => read(kind, path));
  };
  const pick = async (kind: FileSource) => {
    const result = await requests.run(async (isCurrent) => {
      const path = (await (kind === "zip" ? actions.onPickZip() : actions.onPickDirectory()))?.trim();
      if (!path || !isCurrent()) return null;
      setPaths((previous) => ({ ...previous, [kind]: path }));
      return read(kind, path);
    });
    return result;
  };
  return { ...requests, paths, changePath, scan, pick };
}
