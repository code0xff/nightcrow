/**
 * Show an HTML file as the document it describes, isolated from this page.
 *
 * Rendering HTML means executing it, and this origin is not a safe place to
 * execute a file out of someone's repository: the terminal WebSocket lives
 * here, so a script that runs at this origin can open a shell on the server
 * with the session already authenticated. Cloning a remote makes that a
 * reachable path, not a theoretical one.
 *
 * So the isolation is the browser's, not ours. An empty `sandbox` keeps every
 * restriction on — no scripts, no same-origin, no forms, no popups, no
 * navigating the top frame — which is a guarantee, where a sanitizer is a
 * blacklist we would have to keep winning. `srcdoc` rather than a URL because
 * the source is already here; it also means no request for the document, and
 * the frame inherits this page's CSP on top of the sandbox.
 *
 * The cost is deliberate and worth stating: a document that needs scripts does
 * not work, and relative stylesheets, images, and links do not resolve —
 * `srcdoc` has no base URL, and giving it one would mean serving repository
 * files to be fetched at this origin. This previews a self-contained page, not
 * a site.
 */
export function HtmlView({ source }: { source: string }) {
  return (
    <iframe
      title="HTML preview"
      sandbox=""
      srcDoc={source}
      // Documents are written for a white page; the app's dark surface would
      // black out any text that does not set its own colour.
      className="h-full w-full border-0 bg-white"
    />
  );
}
