use crate::web::viewer::dto::Envelope;
use anyhow::{Context, Result};

pub(in crate::web::viewer::server) fn required_path(
    head: &crate::web::common::http::RequestHead,
) -> Result<String> {
    head.query_param("path")
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing path parameter"))
}

/// An oid query parameter that may be absent, but must parse when present.
///
/// Absent and malformed are kept apart deliberately: silently walking from HEAD
/// after a typo would answer a different question than the one asked.
pub(in crate::web::viewer::server) fn optional_oid(
    head: &crate::web::common::http::RequestHead,
    name: &str,
) -> Result<Option<git2::Oid>> {
    match head.query_param(name) {
        None => Ok(None),
        Some(text) => git2::Oid::from_str(&text)
            .map(Some)
            .with_context(|| format!("malformed {name} parameter")),
    }
}

/// A non-negative count query parameter, defaulting to zero when absent.
/// Deliberately unbounded -- see the note beside [`crate::web::viewer::limits::MAX_LOG_PAGE`].
pub(in crate::web::viewer::server) fn optional_count(
    head: &crate::web::common::http::RequestHead,
    name: &str,
) -> Result<usize> {
    let Some(text) = head.query_param(name) else {
        return Ok(0);
    };
    text.parse()
        .with_context(|| format!("malformed {name} parameter"))
}

pub(in crate::web::viewer::server) fn required_oid(
    head: &crate::web::common::http::RequestHead,
) -> Result<git2::Oid> {
    let oid_text = head
        .query_param("oid")
        .ok_or_else(|| anyhow::anyhow!("missing oid parameter"))?;
    git2::Oid::from_str(&oid_text).context("malformed oid")
}

pub(in crate::web::viewer::server) fn encode<T: serde::Serialize>(payload: &T) -> Result<String> {
    serde_json::to_string(&Envelope::new(payload)).context("failed to encode payload")
}
