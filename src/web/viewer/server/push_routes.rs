//! `/api/push/*`: the viewer's half of Web Push.
//!
//! Every route but the key is a POST, for the CSRF reasoning in `dispatch`.
//! An endpoint is a capability URL, so it travels in a body, never a query.

use super::ViewerState;
use super::http_util::{encode, json_error, json_response};
use crate::web::common::http::RequestHead;
use crate::web::viewer::push::Subscription;

#[derive(serde::Deserialize)]
struct EndpointRequest {
    endpoint: String,
}

pub(super) fn handle(head: &RequestHead, body: &str, state: &ViewerState) -> Vec<u8> {
    let Some(push) = state.push.as_ref() else {
        return json_error("503 Service Unavailable", "notifications are unavailable");
    };
    match (head.method.as_str(), head.path.as_str()) {
        ("GET", "/api/push/key") => enveloped(
            "200 OK",
            serde_json::json!({ "public_key": push.public_key() }),
        ),
        ("POST", "/api/push/subscribe") => match serde_json::from_str::<Subscription>(body) {
            Ok(subscription) => match push.subscribe(subscription) {
                Ok(()) => enveloped("200 OK", serde_json::json!({ "subscribed": true })),
                Err(problem) => json_error("400 Bad Request", problem),
            },
            Err(_) => json_error("400 Bad Request", "expected a push subscription"),
        },
        ("POST", "/api/push/unsubscribe") => with_endpoint(body, |endpoint| {
            push.unsubscribe(endpoint);
            enveloped("200 OK", serde_json::json!({ "subscribed": false }))
        }),
        ("POST", "/api/push/status") => with_endpoint(body, |endpoint| {
            enveloped(
                "200 OK",
                serde_json::json!({ "subscribed": push.is_subscribed(endpoint) }),
            )
        }),
        ("POST", "/api/push/test") => with_endpoint(body, |endpoint| {
            if push.send_test(endpoint) {
                enveloped("202 Accepted", serde_json::json!({ "queued": true }))
            } else {
                json_error("404 Not Found", "this browser is not subscribed")
            }
        }),
        _ => json_error("404 Not Found", "not found"),
    }
}

fn with_endpoint(body: &str, act: impl FnOnce(&str) -> Vec<u8>) -> Vec<u8> {
    match serde_json::from_str::<EndpointRequest>(body) {
        Ok(request) => act(&request.endpoint),
        Err(_) => json_error("400 Bad Request", "expected a JSON body with an endpoint"),
    }
}

/// Every viewer response carries the protocol version (`dto::Envelope`).
fn enveloped(status: &str, payload: serde_json::Value) -> Vec<u8> {
    match encode(&payload) {
        Ok(body) => json_response(status, &body, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode response"),
    }
}
