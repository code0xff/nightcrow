//! What `assets.rs` promises: the shell carries its build, a miss is told apart
//! from a route, and nothing outside the bundle can be reached.

use super::*;

fn text(response: &[u8]) -> String {
    String::from_utf8_lossy(response).into_owned()
}

#[test]
fn the_frontend_is_embedded() {
    assert!(
        is_present(),
        "viewer-ui/dist must be committed and built; run `npm --prefix viewer-ui run build`"
    );
}

#[test]
fn the_build_id_names_this_build_and_keeps_naming_it() {
    // Stable across calls, because the client reads it as "the build that
    // answered", and a value that moved on its own would ask every page to
    // reload on every poll.
    let id = build_id().expect("a built frontend has an id");

    assert_eq!(
        id.len(),
        BUILD_ID_BYTES * 2,
        "hex of {BUILD_ID_BYTES} bytes"
    );
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(build_id(), Some(id));
}

#[test]
fn the_shell_says_which_build_it_is_part_of() {
    // The one fact an API response cannot supply: a response arrives after
    // the document and may come from a server that has been replaced since.
    let id = build_id().expect("a built frontend has an id");
    let text = text(&serve("/").unwrap());

    assert!(
        text.contains(&format!("<meta name=\"{BUILD_META}\" content=\"{id}\" />")),
        "the shell must carry its build id"
    );
}

#[test]
fn a_client_side_route_gets_the_stamped_shell_too() {
    // The shell reaches a page by two paths, and a page loaded by the other
    // one needs the same fact.
    let text = text(&serve("/some/route").unwrap());

    assert!(text.contains(BUILD_META), "expected the stamped shell");
}

#[test]
fn a_document_with_no_head_is_served_as_it_is() {
    // Not this server's shell. Better to serve it whole and have the page
    // say nothing than to guess where the stamp goes.
    let plain = b"<html><body>hi</body></html>";

    assert_eq!(stamp_build(plain, "3f6a1c04"), plain.to_vec());
}

#[test]
fn the_root_serves_the_app_shell() {
    let response = serve("/").expect("index.html");
    let text = text(&response);

    assert!(text.starts_with("HTTP/1.1 200 OK"));
    assert!(text.contains("text/html"));
    assert!(text.contains("<div id=\"root\">"), "not the app shell");
}

#[test]
fn assets_carry_a_strict_csp_and_nosniff() {
    let text = text(&serve("/").unwrap());

    assert!(text.contains("Content-Security-Policy: default-src 'self'"));
    assert!(text.contains("frame-ancestors 'none'"));
    assert!(text.contains("X-Content-Type-Options: nosniff"));
}

#[test]
fn a_traversal_request_cannot_escape_the_embedded_map() {
    // There is no filesystem lookup here, so a `..` simply misses and the
    // app shell is served instead of anything outside the bundle.
    let text = text(&serve("/../../etc/passwd").unwrap());

    assert!(text.contains("<div id=\"root\">"), "expected the app shell");
    assert!(!text.contains("root:x:"), "a system file leaked");
}

#[test]
fn a_missing_asset_is_a_404_not_the_app_shell() {
    // A named file that is not embedded must 404, not fall back to
    // index.html: serving HTML under an <img>/module request fails silently
    // (the mark rendered as a blank accent tile when a stale build lacked
    // crow-mono.svg). A loud 404 surfaces the missing asset instead.
    let text = text(&serve("/crow-mono-does-not-exist.svg").unwrap());

    assert!(text.starts_with("HTTP/1.1 404"), "got: {text}");
    assert!(
        !text.contains("<div id=\"root\">"),
        "must not serve the shell"
    );
}

#[test]
fn an_embedded_svg_asset_is_served_as_an_image() {
    // The crow mark's source: present in the bundle and served with an image
    // type, so the <img> actually renders it.
    let text = text(&serve("/crow-mono.svg").unwrap());

    assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
    assert!(text.contains("image/svg+xml"), "wrong content type");
}

#[test]
fn an_extensionless_route_falls_back_to_the_shell() {
    // A client-side route (no file extension) still gets the app shell so
    // the SPA loads; only named-file misses 404.
    let text = text(&serve("/some/route").unwrap());

    assert!(text.contains("<div id=\"root\">"), "expected the app shell");
}

#[test]
fn the_pwa_manifest_is_served_as_a_manifest() {
    // The install manifest must be reachable and typed as JSON so the
    // browser parses it rather than downloading it as an opaque blob.
    let text = text(&serve("/manifest.webmanifest").unwrap());

    assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
    assert!(
        text.contains("json"),
        "manifest served with a non-JSON type"
    );
}

#[test]
fn a_pwa_icon_is_served_as_a_png() {
    // Home-screen install needs raster icons the launcher can render.
    let text = text(&serve("/icon-512.png").unwrap());

    assert!(text.starts_with("HTTP/1.1 200"), "got: {text}");
    assert!(text.contains("image/png"), "wrong content type");
}

#[test]
fn a_javascript_bundle_is_served_with_a_script_mime() {
    // The build hashes the filename, so find it rather than hard-coding.
    let name = Assets::iter()
        .find(|f| f.ends_with(".js"))
        .expect("a built bundle");
    let text = text(&serve(&format!("/{name}")).unwrap());

    assert!(
        text.contains("javascript"),
        "a script served as the wrong type will be refused by the CSP"
    );
}
