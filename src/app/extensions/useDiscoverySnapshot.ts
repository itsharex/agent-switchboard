import { useCallback, useRef, useState } from "react";
import { discoverExtensions, type CommandError, type ExtensionDiscovery } from "../../api/client";

export function useDiscoverySnapshot(onError: (error: CommandError) => void) {
  const [snapshot, setSnapshot] = useState<ExtensionDiscovery | null>(null);
  const [scanning, setScanning] = useState(false);
  const [stale, setStale] = useState(false);
  const started = useRef(false);
  const inFlight = useRef(false);

  const scan = useCallback(async () => {
    if (inFlight.current) return null;
    inFlight.current = true;
    setScanning(true);
    try {
      const next = await discoverExtensions();
      setSnapshot(next);
      setStale(false);
      return next;
    } catch (caught) {
      setStale(true);
      onError(caught as CommandError);
      return null;
    } finally {
      inFlight.current = false;
      setScanning(false);
    }
  }, [onError]);

  const ensureInitialScan = useCallback(() => {
    if (started.current || snapshot !== null) return;
    started.current = true;
    void scan();
  }, [scan, snapshot]);

  return { snapshot, scanning, stale, scan, ensureInitialScan };
}
