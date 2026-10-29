import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./pages/App";
import { ShortcutIntentProvider } from "./hooks/shortcuts/shortcutIntents";
import { ErrorBoundary } from "./components/feedback/ErrorBoundary";
import { Toaster } from "./components/feedback/Toaster";
import { notePageBuild } from "./lib/runtime/viewerBuild";
import { observeVisualViewport } from "./lib/ui/visualViewport";
import { applyScreenScale, readScreenScale } from "./hooks/ui/screenScale";
import { api } from "./api";
import { openNoticeTarget } from "./lib/notifications/noticeTarget";
import { browserPushEnvironment, registerServiceWorker } from "./lib/notifications/webPush";
import "./styles/index.css";

// The layout viewport stays tall when a mobile soft keyboard opens. Keep the
// root grid tied to the visible viewport so the terminal key bar moves above
// the keyboard. Browsers without visualViewport keep the CSS 100% fallback.
observeVisualViewport(document.documentElement, window);
applyScreenScale(readScreenScale());

// Read here, where the document is: the server stamps the build into the shell
// it serves, and everything downstream compares against it rather than against
// a guess. Absent when the shell did not come from this server — `npm run dev`
// serves it from Vite — and then nothing is claimed.
notePageBuild(
  document
    .querySelector('meta[name="nightcrow-build"]')
    ?.getAttribute("content") || null,
);

// The service worker delivers Web Push and opens the page on the terminal a
// tapped notification named; that target is applied before the first poll.
registerServiceWorker(browserPushEnvironment());
await openNoticeTarget(window.location, window.history, api.setActiveRepo);

// The terminal measures its cell from the face it is given, once, when it
// opens — and the startup handshake sends that measurement to the server. A
// face still loading measures as its fallback, so the first render waits for
// it, briefly: past the timeout the page draws anyway, in the fallback.
const FONT_WAIT_MS = 1500;
await Promise.race([
  document.fonts?.load('400 12px "JetBrains Mono"').catch(() => undefined),
  new Promise((resolve) => setTimeout(resolve, FONT_WAIT_MS)),
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {/*
      The backstop. The inner boundaries keep a failed pane to itself; this one
      is what stands between any error they do not cover and a blank page.
      Outside it, the Toaster stays mounted so anything already reported is
      still readable.
    */}
    <ErrorBoundary>
      {/*
        Above `App` because the terminal panel registers its commands from deep
        inside the tree while the one keyboard handler sits at the top: the
        provider is the seam between them. Outside `App` for the ordinary
        reason a context provider is — a hook cannot read a provider its own
        component renders.
      */}
      <ShortcutIntentProvider>
        <App />
      </ShortcutIntentProvider>
    </ErrorBoundary>
    <Toaster />
  </StrictMode>,
);
