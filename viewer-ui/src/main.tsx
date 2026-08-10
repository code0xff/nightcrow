import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./pages/App";
import { ErrorBoundary } from "./components/feedback/ErrorBoundary";
import { Toaster } from "./components/feedback/Toaster";
import "./styles/index.css";

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
