use super::super::ViewerState;
use super::super::http_util::json_error;
use crate::session;
use crate::web::common::http::RequestHead;

#[derive(serde::Deserialize)]
struct OpenRequest {
    path: String,
}

#[derive(serde::Deserialize)]
struct ReorderRequest {
    order: Vec<String>,
}

/// Open a repository from the browser and add it to the served catalog. The
/// path is user-supplied but the response is public, so a bad path yields a
/// generic message.
pub(in crate::web::viewer::server) fn handle_open_repo(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: OpenRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with a path"),
    };
    match session::open_repo(state.session(), &request.path) {
        Ok(repo) => super::encode_response(
            serde_json::json!({ "repo": crate::web::viewer::dto::RepoDto::from(repo) }),
            "could not encode repository",
        ),
        Err(session::OpenError::EmptyPath) => json_error("400 Bad Request", "a path is required"),
        Err(session::OpenError::NotADirectory) => {
            json_error("400 Bad Request", "no such directory")
        }
        Err(session::OpenError::TooMany) => json_error(
            "409 Conflict",
            "the maximum number of repositories is already open",
        ),
    }
}

/// Close a repository named by the `repo` id and return the updated set.
pub(in crate::web::viewer::server) fn handle_close_repo(
    head: &RequestHead,
    state: &ViewerState,
) -> Vec<u8> {
    let Some(id) = head.query_param("repo") else {
        return json_error("400 Bad Request", "missing repo parameter");
    };
    match session::close_repo(state.session(), &id) {
        Ok(()) => encode_repos(state),
        Err(session::CloseError::UnknownRepo) => json_error("404 Not Found", "unknown repository"),
    }
}

pub(in crate::web::viewer::server) fn handle_reorder_repos(
    body: &str,
    state: &ViewerState,
) -> Vec<u8> {
    let request: ReorderRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with an order"),
    };
    session::reorder_repos(state.session(), &request.order);
    encode_repos(state)
}

fn encode_repos(state: &ViewerState) -> Vec<u8> {
    let repos: Vec<crate::web::viewer::dto::RepoDto> = session::list_repos(state.session())
        .into_iter()
        .map(Into::into)
        .collect();
    super::encode_response(
        serde_json::json!({ "repos": repos }),
        "could not encode repositories",
    )
}
