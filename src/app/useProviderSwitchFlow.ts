import { useCallback, useRef, useState } from "react";
import { previewSwitch, type CommandError, type FilePreview } from "../api/client";

interface ProviderSwitchFlowDeps {
  busy: boolean;
  onTargetProfileChange?: (id: string) => void;
  onError: (error: CommandError) => void;
  clearError?: () => void;
}

export interface ActivationCandidate {
  kind: "activation";
  profileId: string;
  file: FilePreview;
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

function useActivationCandidateState() {
  const [activationCandidate, setActivationCandidate] = useState<ActivationCandidate | null>(null);
  const version = useRef(0);
  const clearCandidates = useCallback(() => {
    version.current += 1;
    setActivationCandidate(null);
  }, []);
  return { activationCandidate, setActivationCandidate, version, clearCandidates };
}

interface ActivationRequestDeps {
  busy: boolean;
  version: { current: number };
  setActivationCandidate: (candidate: ActivationCandidate | null) => void;
  requestPreview: (profileId: string) => Promise<FilePreview>;
  clearRequests: () => void;
  onTargetProfileChange?: (id: string) => void;
  onError: (error: CommandError) => void;
  clearError?: () => void;
}

function useActivationRequests({
  busy,
  version,
  setActivationCandidate,
  requestPreview,
  clearRequests,
  onTargetProfileChange,
  onError,
  clearError,
}: ActivationRequestDeps) {
  const request = useCallback(async (profileId: string) => {
    if (busy) return;
    const requestVersion = ++version.current;
    setActivationCandidate(null);
    clearRequests();
    onTargetProfileChange?.(profileId);
    clearError?.();
    try {
      const file = await requestPreview(profileId);
      if (version.current === requestVersion) {
        setActivationCandidate({ kind: "activation", profileId, file });
      }
    } catch (caught) {
      if (version.current === requestVersion) onError(caught as CommandError);
    }
  }, [
    busy,
    clearError,
    clearRequests,
    onError,
    onTargetProfileChange,
    requestPreview,
    setActivationCandidate,
    version,
  ]);

  const requestActivation = useCallback((profile: { id: string }) => {
    void request(profile.id);
  }, [request]);
  return { requestActivation };
}

/** Only an explicit activation can produce the candidate that authorizes a switch. */
export function useProviderSwitchFlow({
  busy,
  onTargetProfileChange,
  onError,
  clearError,
}: ProviderSwitchFlowDeps) {
  const { requestPreview, clearRequests } = usePreviewRequests();
  const state = useActivationCandidateState();
  const setTargetProfile = useCallback((profileId: string) => {
    state.clearCandidates();
    onTargetProfileChange?.(profileId);
    clearError?.();
  }, [clearError, onTargetProfileChange, state.clearCandidates]);
  const invalidateCandidates = useCallback(() => {
    clearRequests();
    state.clearCandidates();
  }, [clearRequests, state.clearCandidates]);
  const { requestActivation } = useActivationRequests({
    busy,
    version: state.version,
    setActivationCandidate: state.setActivationCandidate,
    requestPreview,
    clearRequests,
    onTargetProfileChange,
    onError,
    clearError,
  });
  return {
    activationCandidate: state.activationCandidate,
    clearCandidates: state.clearCandidates,
    cancelActivation: state.clearCandidates,
    invalidateCandidates,
    setTargetProfile,
    requestActivation,
  };
}
