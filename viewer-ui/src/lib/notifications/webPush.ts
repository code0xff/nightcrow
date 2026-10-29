import { pushApi } from "../../api/push";

/** Where this browser stands with Web Push, as the settings menu shows it. */
export type PushState =
  /** No service worker or Push API in this browser. */
  | "unsupported"
  /** Plain http on a name other than localhost: browsers refuse push there. */
  | "insecure"
  /** iOS allows push only to a page opened from the home screen. */
  | "needs-install"
  /** The person blocked notifications; only the browser's settings undo it. */
  | "denied"
  /** The daemon could not load its push key. */
  | "unavailable"
  | "off"
  | "on";

/** The browser features this module reads, injectable for tests. */
export interface PushEnvironment {
  secure: boolean;
  serviceWorker: ServiceWorkerContainer | undefined;
  hasPushManager: boolean;
  permission: () => NotificationPermission | undefined;
  requestPermission: () => Promise<NotificationPermission>;
  isIos: boolean;
  standalone: boolean;
}

export function browserPushEnvironment(): PushEnvironment {
  const nav = navigator as Navigator & { standalone?: boolean };
  return {
    secure: window.isSecureContext,
    serviceWorker: "serviceWorker" in navigator ? navigator.serviceWorker : undefined,
    hasPushManager: "PushManager" in window,
    permission: () =>
      typeof Notification === "undefined" ? undefined : Notification.permission,
    requestPermission: () => Notification.requestPermission(),
    // iPadOS reports itself as a Mac; touch points tell them apart.
    isIos:
      /iPhone|iPad|iPod/.test(navigator.userAgent) ||
      (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1),
    standalone:
      nav.standalone === true ||
      window.matchMedia?.("(display-mode: standalone)").matches === true,
  };
}

/** Why push cannot be offered at all, before any network request. */
function blocker(env: PushEnvironment): PushState | null {
  if (!env.secure) return "insecure";
  if (env.isIos && !env.standalone) return "needs-install";
  if (!env.serviceWorker || !env.hasPushManager || env.permission() === undefined) {
    return "unsupported";
  }
  if (env.permission() === "denied") return "denied";
  return null;
}

/** Register the worker this module relies on. Safe to call on every load. */
export function registerServiceWorker(env: PushEnvironment): void {
  if (!env.secure || !env.serviceWorker) return;
  env.serviceWorker.register("./sw.js").catch(() => {
    // Push stays "unsupported" in effect; nothing else depends on the worker.
  });
}

async function currentSubscription(
  env: PushEnvironment,
): Promise<PushSubscription | null> {
  const registration = await env.serviceWorker!.ready;
  return registration.pushManager.getSubscription();
}

/** This browser's state, reconciled with the daemon: a subscription the
 *  daemon no longer holds (it dropped it after the push service said it was
 *  gone) is dropped here too, so the menu never claims "on" falsely. */
export async function readPushState(env: PushEnvironment): Promise<PushState> {
  const blocked = blocker(env);
  if (blocked) return blocked;
  const subscription = await currentSubscription(env);
  if (!subscription) return "off";
  if (await pushApi.status(subscription.endpoint)) return "on";
  await subscription.unsubscribe().catch(() => false);
  return "off";
}

export async function enablePush(env: PushEnvironment): Promise<PushState> {
  const blocked = blocker(env);
  if (blocked) return blocked;
  // Asked only from the person's own click: a prompt on page load is the one
  // most people dismiss for good.
  const permission = await env.requestPermission();
  if (permission === "denied") return "denied";
  if (permission !== "granted") return "off";
  let key: string;
  try {
    key = await pushApi.key();
  } catch {
    return "unavailable";
  }
  const registration = await env.serviceWorker!.ready;
  // A subscription made for another daemon key cannot be reused.
  await (await registration.pushManager.getSubscription())?.unsubscribe();
  const subscription = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: base64UrlToBytes(key),
  });
  await pushApi.subscribe(subscription.toJSON());
  return "on";
}

export async function disablePush(env: PushEnvironment): Promise<PushState> {
  const subscription = await currentSubscription(env);
  if (subscription) {
    await pushApi.unsubscribe(subscription.endpoint).catch(() => undefined);
    await subscription.unsubscribe().catch(() => false);
  }
  return blocker(env) ?? "off";
}

export async function sendTestPush(env: PushEnvironment): Promise<void> {
  const subscription = await currentSubscription(env);
  if (!subscription) throw new Error("notifications are off in this browser");
  await pushApi.test(subscription.endpoint);
}

export function base64UrlToBytes(value: string): Uint8Array<ArrayBuffer> {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = base64 + "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(padded);
  const bytes = new Uint8Array(new ArrayBuffer(binary.length));
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}
