use super::super::ViewerState;
use super::super::http_util::json_error;

/// Re-read `config.toml` and report what was applied.
///
/// The body is ignored and no configuration is accepted from the request: the
/// file on the server's disk is what is read. Deciding who may ask happened
/// before this — the route is behind the same session cookie as every other
/// mutation.
pub(in crate::web::viewer::server) fn handle_reload_config(state: &ViewerState) -> Vec<u8> {
    match crate::session::reload::reload_config(state.session()) {
        Ok(report) => super::encode_response(
            serde_json::json!({ "summary": report.summary() }),
            "could not encode the report",
        ),
        // A refusal is the operator's own message and names the offending key
        // in their config. The request was valid; the on-disk file was not.
        Err(err) => json_error("422 Unprocessable Entity", &err.to_string()),
    }
}
