//! HTTP translation for the session-owned clone service.

use super::ViewerState;
use super::http_util::{json_error, json_response};
use crate::session::{CloneJobState, CloneLookup, CloneStartError};
use crate::web::common::http::RequestHead;
use crate::web::viewer::dto::Envelope;
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct CloneRequest {
    path: String,
    url: String,
}

/// Keep the browser's long-standing request/response shape while delegating admission to session.
pub(super) fn handle_clone(body: &str, state: &Arc<ViewerState>) -> Vec<u8> {
    let request: CloneRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a path and a url",
            );
        }
    };
    match crate::session::start_clone(&state.session, &request.path, &request.url) {
        Ok(started) => encode(
            "200 OK",
            serde_json::json!({ "job": started.job, "name": started.name }),
        ),
        Err(error) => {
            let status = match error {
                CloneStartError::AlreadyRunning | CloneStartError::ExistingDestination => {
                    "409 Conflict"
                }
                CloneStartError::StartWorker => "500 Internal Server Error",
                _ => "400 Bad Request",
            };
            json_error(status, error.message())
        }
    }
}

/// Poll a particular job, or discover the currently running one after a refresh.
pub(super) fn handle_clone_status(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    let job = match head.query_param("job") {
        Some(raw) => match raw.parse::<u64>() {
            Ok(id) => Some(id),
            Err(_) => return json_error("400 Bad Request", "a job id must be a number"),
        },
        None => None,
    };
    let payload = match crate::session::clone_status(&state.session, job) {
        CloneLookup::Idle => serde_json::json!({ "job": null }),
        CloneLookup::Unknown { .. } => return json_error("404 Not Found", "no such clone"),
        CloneLookup::Found {
            job: id,
            state: CloneJobState::Running,
        } => {
            if job.is_none() {
                serde_json::json!({ "job": id })
            } else {
                serde_json::json!({ "state": "running" })
            }
        }
        CloneLookup::Found {
            state: CloneJobState::Done { path },
            ..
        } => {
            serde_json::json!({ "state": "done", "path": path })
        }
        CloneLookup::Found {
            state: CloneJobState::Failed { message },
            ..
        } => {
            serde_json::json!({ "state": "failed", "message": message })
        }
    };
    encode("200 OK", payload)
}

fn encode(status: &str, payload: serde_json::Value) -> Vec<u8> {
    match serde_json::to_string(&Envelope::new(payload)) {
        Ok(json) => json_response(status, &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode the clone"),
    }
}
