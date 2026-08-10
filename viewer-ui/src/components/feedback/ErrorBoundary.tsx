import { Component, type ErrorInfo, type ReactNode } from "react";
import { isChunkLoadError } from "../../lib/chunkError";

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
  /// Display and visibility for the fallback, which stands where the child
  /// stood and inherits none of its classes. A panel the layout hides at this
  /// width must stay hidden when it fails, or failing is how it appears.
  /// Carries the display class itself (default `flex`), so a caller passing
  /// `hidden md:flex` is not fighting a `flex` baked in here — which class wins
  /// would then be CSS source order, not the call site's intent.
  className?: string;
}

interface State {
  error: unknown;
  /// Separate from `error` because `throw null` is legal: what was thrown does
  /// not say whether anything was.
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
        chunk={isChunkLoadError(this.state.error)}
        region={this.props.region}
        className={this.props.className}
      />
    );
  }
}

function Fallback({
  chunk,
  region,
  className,
}: {
  chunk: boolean;
  region?: string;
  className?: string;
}) {
  return (
    <div
      role="alert"
      className={`h-full min-h-0 flex-col items-start gap-3 p-4 text-ink-400 ${className ?? "flex"}`}
    >
      {chunk ? (
        <>
          <p className="text-accent">Part of the app could not be loaded.</p>
          {/*
            Which of the two it is cannot be known from here — a removed chunk
            and an unreachable server arrive as the same error — so both are
            named, likeliest first, and the reload settles it either way.
          */}
          <p>
            Most likely the server was updated while this tab was open, and
            reloading picks up the current version. If the reload fails too, the
            server is not reachable from here. Either way nothing on the server
            is affected — the session, its repositories, and its terminals are
            untouched.
          </p>
        </>
      ) : (
        <>
          <p className="text-removed">
            {region
              ? `The ${region} could not be rendered.`
              : "Something went wrong."}
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
