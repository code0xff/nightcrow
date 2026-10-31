import { useCallback, useEffect, useState } from "react";
import {
  browserPushEnvironment,
  disablePush,
  enablePush,
  readPushState,
  sendTestPush,
  type PushEnvironment,
  type PushState,
} from "../../lib/notifications/webPush";
import { toast } from "../../lib/runtime/toast";

/** This browser's Web Push state, read when the caller mounts, and the
 *  actions that change it. `null` until the first read settles. */
export function usePushNotifications(
  env: PushEnvironment = browserPushEnvironment(),
) {
  const [state, setState] = useState<PushState | null>(null);
  const [busy, setBusy] = useState(false);
  // The environment is fixed for the page's life; a new object per render
  // must not re-run the read.
  const [environment] = useState(env);

  useEffect(() => {
    let cancelled = false;
    readPushState(environment)
      .then((next) => !cancelled && setState(next))
      .catch(() => !cancelled && setState("off"));
    return () => {
      cancelled = true;
    };
  }, [environment]);

  const run = useCallback(
    async (action: () => Promise<PushState | void>, failure: string) => {
      setBusy(true);
      try {
        const next = await action();
        if (next) setState(next);
        return true;
      } catch (err) {
        toast.error(err instanceof Error ? `${failure}: ${err.message}` : failure);
        return false;
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  return {
    state,
    busy,
    enable: () => run(() => enablePush(environment), "could not turn notifications on"),
    disable: () => run(() => disablePush(environment), "could not turn notifications off"),
    test: async () => {
      const sent = await run(() => sendTestPush(environment), "could not send a test");
      if (sent) toast.info("Test notification sent");
    },
  };
}
