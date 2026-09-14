import { useEffect, useRef, useState } from "react";
import type { ExtensionPlanView, PlanRequest } from "../../api/client";

type Confirmation =
  | { kind: "scope"; request: PlanRequest }
  | { kind: "write"; plan: ExtensionPlanView };

export function useExtensionApplyConfirmation() {
  const [prompt, setPrompt] = useState<Confirmation | null>(null);
  const [sharedSettings, setShared] = useState<boolean | null>(null);
  const scope = useRef<boolean | null>(null);
  const pending = useRef<{
    kind: Confirmation["kind"];
    settle: (answer: boolean | null) => void;
  } | null>(null);

  useEffect(() => () => pending.current?.settle(null), []);

  const ask = (question: Confirmation, signal: AbortSignal): Promise<boolean | null> => {
    if (signal.aborted) return Promise.resolve(null);
    return new Promise((resolve) => {
      const abort = () => settle(null);
      const settle = (answer: boolean | null) => {
        signal.removeEventListener("abort", abort);
        pending.current = null;
        if (!signal.aborted) setPrompt(null);
        resolve(answer);
      };
      pending.current = { kind: question.kind, settle };
      signal.addEventListener("abort", abort, { once: true });
      scope.current = null;
      setShared(null);
      setPrompt(question);
    });
  };
  const answer = (kind: Confirmation["kind"], value: boolean | null) => {
    if (pending.current?.kind === kind) pending.current.settle(value);
  };

  return {
    requestScope: (request: PlanRequest, signal: AbortSignal) => ask({ kind: "scope", request }, signal),
    requestWrite: (plan: ExtensionPlanView, signal: AbortSignal) => ask({ kind: "write", plan }, signal),
    dialog: {
      pendingDisable: prompt?.kind === "scope" ? prompt.request : null,
      pendingWrite: prompt?.kind === "write" ? { plan: prompt.plan } : null,
      sharedSettings,
      setSharedSettings: (value: boolean) => { scope.current = value; setShared(value); },
      confirmDisableScope: () => {
        if (scope.current !== null) answer("scope", scope.current);
      },
      cancelDisableScope: () => answer("scope", null),
      confirmPendingWrite: () => answer("write", true),
      cancelPendingWrite: () => answer("write", null),
    },
  };
}
