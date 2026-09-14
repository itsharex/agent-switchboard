import { useCallback, useRef, useState } from "react";
import { previewSwitch, type CommandError, type FilePreview } from "../api/client";

interface SwitchPreviewDeps {
  busy: boolean;
  setSelectedId: (id: string) => void;
  onError: (error: CommandError) => void;
  clearError?: () => void;
}

export interface SwitchCandidate {
  profileId: string;
  file: FilePreview;
  intent: "preview" | "activate";
}

function usePreviewRequests() {
  const requests = useRef(new Map<string, Promise<FilePreview>>());
  const requestPreview = useCallback((profileId: string) => {
    const existing = requests.current.get(profileId);
    if (existing) return existing;
    const next = previewSwitch(profileId);
    requests.current.set(profileId, next);
    void next.finally(() => {
      if (requests.current.get(profileId) === next) requests.current.delete(profileId);
    }).catch(() => {});
    return next;
  }, []);
  const clearRequests = useCallback(() => requests.current.clear(), []);
  return { requestPreview, clearRequests };
}

/** Both clients bind write intent to one exact candidate, never to a later preview. */
export function useSwitchPreview({ busy, setSelectedId, onError, clearError }: SwitchPreviewDeps) {
  const [preview, setPreview] = useState<SwitchCandidate | null>(null);
  const version = useRef(0);
  const { requestPreview, clearRequests } = usePreviewRequests();

  const retractPreview = useCallback(() => {
    version.current += 1;
    setPreview(null);
  }, []);

  const selectProfile = useCallback((profileId: string) => {
    retractPreview();
    setSelectedId(profileId);
    clearError?.();
  }, [clearError, retractPreview, setSelectedId]);

  const invalidateSwitchCandidates = useCallback(() => {
    clearRequests();
    retractPreview();
  }, [clearRequests, retractPreview]);

  const loadProfile = useCallback(async (profileId: string, intent: SwitchCandidate["intent"]) => {
    if (busy) return;
    const requestedVersion = ++version.current;
    if (intent === "activate") clearRequests();
    setSelectedId(profileId);
    setPreview(null);
    clearError?.();
    try {
      const file = await requestPreview(profileId);
      if (version.current === requestedVersion) setPreview({ profileId, file, intent });
    } catch (caught) {
      if (version.current === requestedVersion) onError(caught as CommandError);
    }
  }, [busy, clearError, clearRequests, onError, requestPreview, setSelectedId]);

  const previewProfile = useCallback(
    (profile: { id: string }) => loadProfile(profile.id, "preview"), [loadProfile],
  );
  const activateProfile = useCallback(
    (profile: { id: string }) => loadProfile(profile.id, "activate"), [loadProfile],
  );
  const togglePreviewProfile = useCallback((profile: { id: string }) => {
    if (preview?.profileId === profile.id) retractPreview();
    else void previewProfile(profile);
  }, [preview, previewProfile, retractPreview]);
  const cancelSwitch = useCallback(() => {
    setPreview((current) => current ? { ...current, intent: "preview" } : null);
  }, []);

  return {
    preview,
    switchCandidate: preview?.intent === "activate" ? preview : null,
    cancelSwitch,
    retractPreview,
    invalidateSwitchCandidates,
    selectProfile,
    previewProfile,
    activateProfile,
    togglePreviewProfile,
  };
}
