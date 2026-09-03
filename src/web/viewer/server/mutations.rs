mod file;
mod filesystem;
mod lookup;
mod preferences;
mod reload;
mod repository;

pub(super) use file::handle_write_file;
pub(super) use filesystem::handle_mkdir;
pub(super) use lookup::{lookup_repo, redact};
pub(super) use preferences::handle_set_prefs;
pub(super) use reload::handle_reload_config;
pub(super) use repository::{handle_close_repo, handle_open_repo, handle_reorder_repos};

use super::http_util::{json_error, json_response};

/// Serialize one successful mutation response through the viewer envelope.
fn encode_response<T: serde::Serialize>(payload: T, error: &'static str) -> Vec<u8> {
    match super::http_util::encode(&payload) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", error),
    }
}
