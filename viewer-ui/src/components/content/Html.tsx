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
 * the source is already here, and the frame inherits this page's CSP on top of
 * the sandbox.
 *
 * What the document *can* still do is load subresources, bounded by that
 * inherited CSP. `img-src 'self' data:` is the one directive that admits a
 * `data:` URI, so a page carrying its images inline renders completely; every
 * other kind of resource falls to `default-src 'self'`, which admits neither a
 * `data:` stylesheet nor any other host — refused as a subresource and as a
 * frame navigation, so a link the user clicks does not leave either (verified
 * in a browser, not reasoned).
 *
 * A relative or root-relative URL does resolve, against this origin: a
 * `srcdoc` document's base URL is the embedder's, not nothing. It therefore
 * reaches this server, which serves the app bundle and the API but never
 * repository files, so the sibling stylesheet a document expects 404s. Those
 * requests are unauthenticated — the sandbox gives the frame an opaque origin
 * and the session cookie is `SameSite=Strict` — so even `GET /logout` from
 * inside the frame leaves the session alone.
 *
 * The cost is deliberate: a document that needs scripts does not work, and one
 * that links its stylesheet or images as separate files shows without them.
 * This previews a self-contained page, not a site.
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
