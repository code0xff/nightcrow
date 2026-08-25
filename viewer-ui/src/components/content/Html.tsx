/**
 * Show an HTML file as the document it describes, isolated from this page.
 *
 * Rendering HTML means executing it, and this origin is not a safe place to
 * execute a file out of someone's repository: the terminal WebSocket lives
 * here, so a script that runs at this origin can open a shell on the server
 * with the session already authenticated. Cloning a remote makes that a
 * reachable path, not a theoretical one.
 *
 * So the isolation is the browser's, not ours — and it is layered twice. The
 * frame loads `/api/preview` by URL rather than inlining the file: a `srcdoc`
 * document inherits this page's CSP, whose `script-src 'self'` refuses the
 * inline scripts a self-contained page is made of, so a slide deck rendered
 * but never ran. A navigated response carries its own policy instead — see
 * `server/preview.rs` for the whole of it: `sandbox allow-scripts` makes the
 * document an opaque origin whose inline scripts run, `connect-src 'none'`
 * closes every outbound channel, and everything it may load is `data:` or
 * refused, so a preview still never loads from another host.
 *
 * What keeps those scripts away from the session: the opaque origin is not
 * this origin (nothing of this page's DOM or storage is reachable); the
 * session cookie is `SameSite=Strict`, so the frame's requests arrive
 * unauthenticated; and its `Origin: null` is refused by the server's origin
 * check before auth is even consulted, the terminal WebSocket included.
 *
 * The `sandbox` attribute here repeats what the response header declares.
 * Deliberate: the two intersect, so either one failing — an old browser, a
 * future edit to one side — still leaves the other standing. Neither ever
 * includes `allow-same-origin`, which together with scripts would be this
 * page.
 *
 * What no policy closes: a script may navigate its own frame away — to an
 * external URL or a phishing page in the pane. That is inherent to allowing
 * scripts (the frame stays opaque and reaches no session), and is an accepted
 * residual. The two navigations that *would* reach the session — a top-level
 * load executing as this origin, a frame self-navigating to `/logout` — are
 * closed server-side by `Sec-Fetch-Dest` (see `server/preview.rs`).
 *
 * The remaining cost: a document that links its stylesheet, images, or
 * scripts as separate files (or from a CDN) shows without them. This previews
 * a self-contained page, not a site.
 */
export function HtmlView({ src }: { src: string }) {
  return (
    <iframe
      title="HTML preview"
      sandbox="allow-scripts"
      src={src}
      // Documents are written for a white page; the app's dark surface would
      // black out any text that does not set its own colour.
      className="h-full w-full border-0 bg-white"
    />
  );
}
