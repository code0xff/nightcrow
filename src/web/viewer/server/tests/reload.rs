//! The browser's reload route.
//!
//! What a reload *applies* is pinned in `web::viewer::reload`, against a temp
//! file. What is left here is the route: that it is gated, that it takes no
//! configuration from the caller, and that a GET is not a way to trigger it.
//!
//! These are started with no repositories, so a reload has no hub to fan out to
//! and cannot launch a plugin child out of whatever config the machine running
//! the tests happens to have.

use super::{get, login, post, server};

#[test]
fn reloading_requires_authentication() {
    let server = server(&[]);

    let response = post(server.addr(), "/api/reload", "", None);

    assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
}

/// A GET must not reload. Not a nicety: a plain link or a prefetch would then be
/// enough to restart the session's plugin children.
#[test]
fn reloading_is_not_reachable_by_a_get() {
    let server = server(&[]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/reload", Some(&token));

    assert!(
        !response.starts_with("HTTP/1.1 200"),
        "a GET must not reload: {response}"
    );
}

/// The body is ignored, so the route cannot be used to hand the session a
/// configuration the caller invented. A body that would be nonsense as config
/// must make no difference to the answer.
#[test]
fn the_request_body_is_not_read_as_configuration() {
    let server = server(&[]);
    let token = login(server.addr());

    let empty = post(server.addr(), "/api/reload", "", Some(&token));
    let smuggled = post(
        server.addr(),
        "/api/reload",
        r#"{"plugins":[{"name":"x","command":"/bin/sh","enabled":true}]}"#,
        Some(&token),
    );

    assert_eq!(
        status_line(&empty),
        status_line(&smuggled),
        "the body must not change what a reload does"
    );
}

fn status_line(response: &str) -> &str {
    response.lines().next().unwrap_or_default()
}
