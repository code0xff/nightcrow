import { Component, type ErrorInfo, type ReactNode } from "react";
import { isStaleBundleError } from "../../lib/chunkError";

/**
 * Keeps one failed subtree from taking the page with it.
 *
 * Without a boundary anywhere, React unmounts the whole tree on any render
 * error and the viewer goes blank — indistinguishable, to the person looking at
 * it, from the server having died. That is the shape a missing chunk arrived in:
 * open an HTML preview against a build the server has replaced, and the page
 * disappears with nothing said. See `lib/chunkError.ts` for how that case is
 * told apart, and why reloading is the only way out of it.
 *
 * A class because this is the one thing React has no hook for — `componentDidCatch`
 * and `getDerivedStateFromError` exist on classes only.
 *
 * The boundary holds its error until it is remounted, which is what `key` is
 * for at the call site: a pane keyed by the file it shows starts clean on the
 * next file, so a dead HTML chunk does not also bury the markdown one.
 */
interface Props {
  children: ReactNode;
  /// Names the region in the fallback, so a failed pane does not read as a
  /// failed application.
  region?: string;
}

interface State {
  error: unknown;
  failed: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, failed: false };

  static getDerivedStateFromError(error: unknown): State {
    return { error, failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    // The console is the only place a stack survives; the fallback deliberately
    // shows none, since nothing the reader can do depends on it.
    console.error("nightcrow: a subtree failed to render", error, info);
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <Fallback
        stale={isStaleBundleError(this.state.error)}
        region={this.props.region}
      />
    );
  }
}

function Fallback({ stale, region }: { stale: boolean; region?: string }) {
  return (
    <div
      role="alert"
      className="flex h-full min-h-0 flex-col items-start gap-3 p-4 text-ink-400"
    >
      {stale ? (
        <>
          <p className="text-accent">A new version was deployed.</p>
          <p>
            This tab is running the older one and cannot load the rest of it.
            Reloading picks up the new version — the session, its repositories,
            and its terminals are on the server and are not affected.
          </p>
        </>
      ) : (
        <>
          <p className="text-removed">
            {region ? `The ${region} stopped responding.` : "Something broke."}
          </p>
          <p>The details are in the browser console.</p>
        </>
      )}
      <button
        onClick={() => window.location.reload()}
        className="rounded-sm border border-ink-700 px-2 py-1 text-ink-200 hover:border-accent hover:text-accent"
      >
        Reload
      </button>
    </div>
  );
}
