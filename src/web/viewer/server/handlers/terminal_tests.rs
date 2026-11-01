//! Who a socket belongs to.

use super::{MAX_VIEWER_ID, browser_viewer};
use crate::session::size_owner::ViewerId;
use crate::web::common::http::RequestHead;

fn head(query: &str) -> RequestHead {
    crate::web::common::http::parse_request_head(&format!(
        "GET /ws/term?{query} HTTP/1.1\r\nHost: h\r\n\r\n"
    ))
    .expect("a well-formed request line")
}

#[test]
fn a_page_that_names_itself_is_taken_at_its_word() {
    assert_eq!(
        browser_viewer(&head("repo=r1&viewer=tab-9f2c")),
        ViewerId::Browser("tab-9f2c".to_string())
    );
}

/// The whole point of the id is that the same page keeps it across sockets.
#[test]
fn the_same_page_is_the_same_viewer_on_every_socket() {
    assert_eq!(
        browser_viewer(&head("repo=r1&viewer=tab-1")),
        browser_viewer(&head("repo=r2&viewer=tab-1")),
    );
}

/// A stale bundle, or something that is not a page at all. It still works --
/// it just behaves as a browser did before it could name itself.
#[test]
fn a_socket_with_no_usable_id_gets_one_of_its_own() {
    let long = "a".repeat(MAX_VIEWER_ID + 1);
    for query in [
        "repo=r1".to_string(),
        "repo=r1&viewer=".to_string(),
        "repo=r1&viewer=has%20space".to_string(),
        "repo=r1&viewer=semi;colon".to_string(),
        format!("repo=r1&viewer={long}"),
    ] {
        let first = browser_viewer(&head(&query));
        let second = browser_viewer(&head(&query));
        assert_ne!(first, second, "each must be its own screen: {query}");
    }
}

#[test]
fn an_id_at_the_cap_is_still_accepted() {
    let id = "a".repeat(MAX_VIEWER_ID);
    assert_eq!(
        browser_viewer(&head(&format!("repo=r1&viewer={id}"))),
        ViewerId::Browser(id)
    );
}
