// The viewer's service worker. It exists for Web Push only: it caches nothing
// and never answers a fetch, so the page loads from the server exactly as it
// would without it.

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

// The daemon encrypts each message to this browser's key; the browser has
// already decrypted it by the time it arrives here.
self.addEventListener("push", (event) => {
  let notice = {};
  try {
    notice = event.data ? event.data.json() : {};
  } catch {
    // An unreadable message still has to show something: a push the page
    // receives without a notification can cost it its subscription.
  }
  event.waitUntil(
    self.registration.showNotification(notice.title || "nightcrow", {
      body: notice.body || "A terminal wants your attention",
      tag: notice.tag || undefined,
      renotify: Boolean(notice.tag),
      icon: "icon-192.png",
      data: { repo: notice.repo, pane: notice.pane },
    }),
  );
});

// Open the viewer on the project and terminal that asked. The page reads
// `notice-repo` and `notice-pane` once at startup (`lib/notifications`), so
// an open page is navigated, not messaged: one path for both cases.
self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const { repo, pane } = event.notification.data || {};
  const target = new URL(self.registration.scope);
  if (typeof repo === "string") {
    target.searchParams.set("notice-repo", repo);
    if (Number.isInteger(pane)) target.searchParams.set("notice-pane", String(pane));
  }
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: "window" });
      const open = windows.find((client) => client.url.startsWith(self.registration.scope));
      if (!open) return self.clients.openWindow(target.href);
      await open.focus();
      if (typeof repo === "string") {
        await open.navigate(target.href).catch(() => self.clients.openWindow(target.href));
      }
    })(),
  );
});
