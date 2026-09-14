import { useEffect, useRef, useState } from "react";
import { sourceErrorMessage } from "./skill-source-model";

export function useSkillSourceRequests<T>(busy: boolean) {
  const generation = useRef(0);
  const active = useRef<number | null>(null);
  const latest = useRef<T | null>(null);
  const [result, setResult] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => { generation.current += 1; active.current = null; }, []);

  const cancel = () => {
    generation.current += 1;
    active.current = null;
    setLoading(false);
    setError(null);
  };
  const update = (transform: (previous: T | null) => T | null) => {
    latest.current = transform(latest.current);
    setResult(latest.current);
  };
  const clear = () => { cancel(); update(() => null); };
  const run = async (read: (isCurrent: () => boolean) => Promise<T | null>) => {
    if (busy || active.current !== null) return null;
    const id = ++generation.current;
    active.current = id;
    const current = () => generation.current === id;
    setLoading(true);
    setError(null);
    try {
      const next = await read(current);
      if (!current()) return null;
      if (next !== null) update(() => next);
      return next;
    } catch (reason) {
      if (current()) {
        const message = sourceErrorMessage(reason, "无法读取来源，请检查地址和网络后重试");
        setError(latest.current !== null ? `${message}；仍显示上次结果` : message);
      }
      return null;
    } finally {
      if (current()) { active.current = null; setLoading(false); }
    }
  };
  return { result, loading, error, setError, clear, cancel, update, run };
}
