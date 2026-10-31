import { usePushNotifications } from "../../hooks/notifications/usePushNotifications";
import type { PushState } from "../../lib/notifications/webPush";

/** Why notifications cannot be turned on here, in the person's terms. */
const EXPLANATIONS: Partial<Record<PushState, string>> = {
  unsupported: "This browser does not support notifications.",
  insecure: "Notifications need HTTPS or localhost. See the notifications guide for tailscale serve.",
  "needs-install": "On iPhone and iPad, add nightcrow to the Home Screen and open it from there first.",
  denied: "Notifications are blocked for this site. Allow them in the browser's site settings.",
  unavailable: "The server could not load its notification key. Check the daemon log.",
};

const ROW_BUTTON =
  "rounded-sm px-2 py-1 text-ink-200 hover:bg-ink-700 disabled:cursor-progress disabled:text-ink-400 disabled:hover:bg-transparent";

/** The settings menu's Notifications section: one switch per browser, plus a
 *  test that proves the whole path, daemon to push service to device. */
export function NotificationSettings() {
  const { state, busy, enable, disable, test } = usePushNotifications();
  const explanation = state ? EXPLANATIONS[state] : undefined;

  return (
    <section aria-labelledby="nc-notifications-title" className="border-t border-ink-800 px-3 py-2">
      <h3 id="nc-notifications-title" className="mb-1 text-xs uppercase tracking-wide text-ink-400">
        Notifications
      </h3>
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 text-ink-200">When a terminal asks for you</span>
        {(state === "off" || state === "on") && (
          <button
            type="button"
            role="switch"
            aria-checked={state === "on"}
            aria-busy={busy}
            disabled={busy}
            onClick={state === "on" ? disable : enable}
            className={ROW_BUTTON}
          >
            {state === "on" ? "On" : "Off"}
          </button>
        )}
        {state === null && <span className="text-ink-400">…</span>}
      </div>
      {explanation && <p className="mt-1 text-xs text-ink-400">{explanation}</p>}
      {state === "on" && (
        <button type="button" disabled={busy} onClick={test} className={`mt-1 ${ROW_BUTTON}`}>
          Send a test notification
        </button>
      )}
    </section>
  );
}
