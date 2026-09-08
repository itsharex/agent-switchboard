import { useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import {
  cancelProviderRequest,
  executeProviderRequest,
  fetchProviderRequestModels,
  prepareProviderRequest,
  type ProviderModel,
  type ProviderRequestTarget,
  type ProviderRequestPreparation,
  type ProviderRequestResult,
} from "../api/client";

export interface RequestView {
  phase: "preparing" | "ready" | "sending" | "cancelling" | "complete" | "failed" | "cancelled";
  preparation: ProviderRequestPreparation | null;
  result: ProviderRequestResult | null;
  error: string | null;
}

interface Session {
  target: ProviderRequestTarget;
  preparation: ProviderRequestPreparation | null;
  busy: boolean;
  cancelled: boolean;
  disposed: boolean;
  attempt: number;
}

type UpdateView = Dispatch<SetStateAction<RequestView>>;
interface ModelListingControls {
  version: MutableRefObject<number>;
  busy: MutableRefObject<boolean>;
  setModels: Dispatch<SetStateAction<ProviderModel[] | null>>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
}

const INITIAL_VIEW: RequestView = { phase: "preparing", preparation: null, result: null, error: null };

function failureMessage(caught: unknown): string {
  if (typeof caught === "object" && caught !== null && "message" in caught && typeof caught.message === "string") {
    return caught.message;
  }
  return typeof caught === "string" ? caught : "请求未完成，请重试。";
}

function release(session: Session) {
  session.disposed = true;
  if (session.preparation) {
    void cancelProviderRequest(session.preparation.requestId).catch((caught: unknown) => {
      console.warn("取消已关闭的供应商请求失败：", failureMessage(caught));
    });
  }
}

async function prepare(session: Session, attempt = session.attempt): Promise<ProviderRequestPreparation | null> {
  const preparation = await prepareProviderRequest(session.target);
  // Preparation may return after cancellation or unmount. Its token must be
  // released before execution can begin, including in React Strict Mode.
  if (session.disposed || session.cancelled || session.attempt !== attempt) {
    await cancelProviderRequest(preparation.requestId);
    return null;
  }
  session.preparation = preparation;
  return preparation;
}

async function send(session: Session, model: string, previous: RequestView, update: UpdateView) {
  if (session.busy || session.disposed || !model.trim()) return;
  session.busy = true;
  session.cancelled = false;
  const attempt = ++session.attempt;
  update({ ...previous, phase: "sending", result: null, error: null });
  try {
    const preparation = session.preparation ?? await prepare(session, attempt);
    if (!preparation || session.disposed || session.attempt !== attempt) return;
    const before = previous.preparation;
    if (before && (before.endpoint !== preparation.endpoint || before.upstreamProtocol !== preparation.upstreamProtocol)) {
      update({ phase: "ready", preparation, result: null, error: "供应商连接已更新，请确认当前地址与 API 格式后再次发送。" });
      return;
    }
    update({ phase: "sending", preparation, result: null, error: null });
    const result = await executeProviderRequest(preparation.requestId, model.trim());
    if (session.disposed || session.attempt !== attempt) return;
    session.preparation = null;
    update({ phase: "complete", preparation, result, error: null });
  } catch (caught) {
    if (session.disposed || session.attempt !== attempt) return;
    const preparation = session.preparation;
    session.preparation = null;
    if (preparation) {
      void cancelProviderRequest(preparation.requestId).catch((error: unknown) => {
        console.warn("释放供应商请求失败：", failureMessage(error));
      });
    }
    update((current) => ({ ...current, phase: "failed", result: null, error: failureMessage(caught) }));
  } finally {
    if (session.attempt === attempt) {
      session.busy = false;
      session.cancelled = false;
    }
  }
}

async function cancel(session: Session, update: UpdateView) {
  if (!session.busy || session.cancelled || session.disposed) return;
  const attempt = session.attempt;
  session.cancelled = true;
  update((current) => ({ ...current, phase: "cancelling", error: null }));
  try {
    const accepted = !session.preparation || await cancelProviderRequest(session.preparation.requestId);
    if (session.disposed || session.attempt !== attempt || !session.busy) return;
    if (!accepted) {
      session.cancelled = false;
      update((current) => ({ ...current, phase: "sending" }));
      return;
    }
    // Cancellation acknowledgement retires this attempt immediately. Its
    // pending execute/finally must never mutate a subsequent request.
    session.attempt += 1;
    session.preparation = null;
    session.busy = false;
    session.cancelled = false;
    update((current) => ({ ...current, phase: "cancelled", result: null, error: null }));
  } catch (caught) {
    if (session.disposed || session.attempt !== attempt || !session.busy) return;
    session.cancelled = false;
    update((current) => ({ ...current, phase: "sending", error: `取消失败：${failureMessage(caught)}` }));
  }
}

async function loadModels(
  sessionRef: MutableRefObject<Session | null>,
  controls: ModelListingControls,
  update: UpdateView,
) {
  const session = sessionRef.current;
  if (controls.busy.current || !session || session.disposed || session.busy) return;
  const version = ++controls.version.current;
  const isCurrent = () => sessionRef.current === session && !session.disposed && controls.version.current === version;
  const hadPreparation = session.preparation !== null;
  controls.busy.current = true;
  controls.setBusy(true);
  controls.setError(null);
  try {
    // A completed send consumes its token. This explicit action may create a
    // new one, but the models command still receives only that token.
    const preparation = session.preparation ?? await prepare(session, session.attempt);
    if (!preparation || !isCurrent()) return;
    if (!hadPreparation) {
      update((current) => {
        if (!isCurrent()) return current;
        const changed = current.preparation
          && (current.preparation.endpoint !== preparation.endpoint
            || current.preparation.upstreamProtocol !== preparation.upstreamProtocol);
        return changed
          ? { phase: "ready", preparation, result: null, error: "供应商连接已更新，请确认当前地址与 API 格式后再发送。" }
          : { ...current, preparation };
      });
    }
    const fetched = await fetchProviderRequestModels(preparation.requestId);
    if (isCurrent()) controls.setModels(fetched);
  } catch (caught) {
    if (isCurrent()) controls.setError(failureMessage(caught));
  } finally {
    if (isCurrent()) {
      controls.busy.current = false;
      controls.setBusy(false);
    }
  }
}

export function useProviderRequest(target: ProviderRequestTarget) {
  const [view, setView] = useState(INITIAL_VIEW);
  const [model, setModel] = useState("");
  const [models, setModels] = useState<ProviderModel[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const sessionRef = useRef<Session | null>(null);
  const modelsVersion = useRef(0);
  const modelsBusyRef = useRef(false);
  const modelListing: ModelListingControls = {
    version: modelsVersion,
    busy: modelsBusyRef,
    setModels,
    setBusy: setModelsBusy,
    setError: setModelsError,
  };

  useEffect(() => {
    const session: Session = { target, preparation: null, busy: false, cancelled: false, disposed: false, attempt: 0 };
    sessionRef.current = session;
    modelsVersion.current += 1;
    modelsBusyRef.current = false;
    setView(INITIAL_VIEW);
    setModel("");
    setModels(null);
    setModelsBusy(false);
    setModelsError(null);
    void prepare(session).then((preparation) => {
      if (!preparation || session.disposed) return;
      setModel(preparation.defaultModel ?? "");
      setView({ phase: "ready", preparation, result: null, error: null });
    }).catch((caught: unknown) => {
      if (session.disposed) {
        console.warn("关闭供应商请求时未能释放准备状态：", failureMessage(caught));
        return;
      }
      setModel("");
      setView({ ...INITIAL_VIEW, phase: "failed", error: failureMessage(caught) });
    });
    return () => {
      modelsVersion.current += 1;
      modelsBusyRef.current = false;
      release(session);
    };
  }, [target, revision]);

  const busy = view.phase === "sending" || view.phase === "cancelling";
  return {
    view,
    model,
    busy,
    models,
    modelsBusy,
    modelsError,
    reload: () => setRevision((value) => value + 1),
    setModel(value: string) {
      setModel(value);
      setView((current) => ({ ...current, phase: "ready", result: null, error: null }));
    },
    fetchModels() {
      return loadModels(sessionRef, modelListing, setView);
    },
    send() {
      if (!modelsBusyRef.current && sessionRef.current) void send(sessionRef.current, model, view, setView);
    },
    cancel() {
      if (sessionRef.current) void cancel(sessionRef.current, setView);
    },
  };
}
