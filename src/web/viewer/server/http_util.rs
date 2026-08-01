use crate::web::common::http;
use crate::web::viewer::dto::Envelope;
use anyhow::{Context, Result};

pub(super) fn encode<T: serde::Serialize>(payload: &T) -> Result<String> {
    serde_json::to_string(&Envelope::new(payload)).context("failed to encode payload")
}

pub(super) fn json_response(status: &str, body: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let mut headers = vec![("X-Content-Type-Options", "nosniff")];
    headers.extend_from_slice(extra);
    http::response(
        status,
        "application/json; charset=utf-8",
        &headers,
        body.as_bytes(),
    )
}

pub(super) fn json_error(status: &str, message: &str) -> Vec<u8> {
    // Message is always a fixed literal from this module, never interpolated
    // from an error, so it needs no escaping and can leak nothing.
    let body = format!("{{\"error\":\"{message}\"}}");
    json_response(status, &body, &[])
}

pub(super) fn text_response(status: &str, message: &str) -> Vec<u8> {
    http::response(
        status,
        "text/plain; charset=utf-8",
        &[("X-Content-Type-Options", "nosniff")],
        message.as_bytes(),
    )
}
