import { useCallback, useState } from "react";
import { useI18n } from "./context";
import { errorText } from "./errors";

/** Keep structured errors and notices until render so visible messages follow language changes. */
export function useMessageState() {
  const [message, setMessage] = useState<unknown>(null);
  const { t } = useI18n();
  const update = useCallback((next: unknown) => setMessage(() => next), []);
  return [message == null ? null : errorText(message, t), update] as const;
}
