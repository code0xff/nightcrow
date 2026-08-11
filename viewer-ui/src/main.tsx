import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./pages/App";
import { ErrorBoundary } from "./components/feedback/ErrorBoundary";
import { Toaster } from "./components/feedback/Toaster";
import { notePageBuild } from "./lib/viewerBuild";
import "./styles/index.css";

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
      <App />
    </ErrorBoundary>
    <Toaster />
  </StrictMode>,
);
