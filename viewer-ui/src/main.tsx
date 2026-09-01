import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./pages/App";
import { ShortcutIntentProvider } from "./hooks/shortcutIntents";
import { ErrorBoundary } from "./components/feedback/ErrorBoundary";
import { Toaster } from "./components/feedback/Toaster";
import { notePageBuild } from "./lib/viewerBuild";
import { observeVisualViewport } from "./lib/visualViewport";
import "./styles/index.css";

// The layout viewport stays tall when a mobile soft keyboard opens. Keep the
// root grid tied to the visible viewport so the terminal key bar moves above
// the keyboard. Browsers without visualViewport keep the CSS 100% fallback.
observeVisualViewport(document.documentElement, window);

// Read here, where the document is: the server stamps the build into the shell
// it serves, and everything downstream compares against it rather than against
// a guess. Absent when the shell did not come from this server — `npm run dev`
// serves it from Vite — and then nothing is claimed.
notePageBuild(
  document
    .querySelector('meta[name="nightcrow-build"]')
    ?.getAttribute("content") || null,
);

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
