use super::super::ViewerState;
use super::super::http_util::json_error;
use crate::session::prefs::{
    MaximizedPanel, MaximizedUpdate, PrefsUpdate, RepoView, ViewFace, ViewFile, ViewTab,
};
use crate::web::viewer::dto::RepoViewDto;

#[derive(serde::Deserialize)]
struct PrefsRequest {
    /// Each preference is optional so one write touches one setting and leaves
    /// the rest as they are.
    accent: Option<usize>,
    sidebar_width: Option<u32>,
    upper_pct: Option<u32>,
    /// Repo **id**, as every other client-supplied repository reference is.
    /// The server translates it to the path `prefs.rs` stores.
    active_repo: Option<String>,
    /// How one project's screen is arranged. Names its own repository rather
    /// than riding on `active_repo`: maximizing is about the project the client
    /// is looking at.
    maximized: Option<MaximizedRequest>,
    /// What one project is showing now, so opening it again opens it. Names its
    /// own repository for the same reason `maximized` does.
    view: Option<ViewRequest>,
}

#[derive(serde::Deserialize)]
struct MaximizedRequest {
    /// Repo id, translated to a path before it is stored.
    repo: String,
    /// `"files"`, `"terminal"`, or absent/null for nothing maximized.
    panel: Option<String>,
}

#[derive(serde::Deserialize)]
struct ViewRequest {
    /// Repo id, translated to a path before it is stored.
    repo: String,
    /// `"status"`, `"log"`, or `"tree"`.
    tab: String,
    file: Option<ViewFileRequest>,
    #[serde(default)]
    tree_expanded: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ViewFileRequest {
    /// Repository-relative path.
    path: String,
    /// Commit id, absent for the working tree's copy.
    #[serde(default)]
    commit: Option<String>,
    /// `"diff"` or `"source"`.
    face: String,
}

/// Store one or more viewer preferences and echo back the full stored set.
///
/// A value with a range is wrapped or clamped into it rather than rejected.
/// `active_repo` is the exception: it names a repository rather than sitting in
/// a range, and there is no nearest valid project to fold an unknown id onto.
pub(in crate::web::viewer::server) fn handle_set_prefs(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: PrefsRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a preference to store",
            );
        }
    };
    if request.accent.is_none()
        && request.sidebar_width.is_none()
        && request.upper_pct.is_none()
        && request.active_repo.is_none()
        && request.maximized.is_none()
        && request.view.is_none()
    {
        return json_error("400 Bad Request", "no known preference in the body");
    }

    let active_path = match request.active_repo {
        Some(id) => match state.session.catalog().get(&id) {
            Some(entry) => Some(entry.path.clone()),
            None => return json_error("400 Bad Request", "unknown repo"),
        },
        None => None,
    };
    let maximized = match request.maximized {
        Some(change) => {
            let Some(entry) = state.session.catalog().get(&change.repo) else {
                return json_error("400 Bad Request", "unknown repo");
            };
            let panel = match change.panel.as_deref() {
                None => None,
                Some(name) => match MaximizedPanel::parse(name) {
                    Some(panel) => Some(panel),
                    None => return json_error("400 Bad Request", "unknown panel"),
                },
            };
            Some(MaximizedUpdate {
                repo: entry.path.clone(),
                panel,
            })
        }
        None => None,
    };

    let view = match request.view {
        Some(view) => match resolve_view(view, state) {
            Ok(view) => Some(view),
            Err(response) => return response,
        },
        None => None,
    };

    // The project in front is shared, so a write that changes it re-points
    // every open page — two pages tugging it back and forth would show here
    // as alternating switches. No-op writes stay silent.
    if let Some(path) = &active_path {
        let before = state.session.prefs().get().active_repo;
        if before.as_deref() != Some(path.as_str()) {
            tracing::info!(from = ?before, to = %path, "viewer: active repo switched");
        }
    }
    let stored = state.session.prefs().update(PrefsUpdate {
        accent: request.accent,
        sidebar_width: request.sidebar_width,
        upper_pct: request.upper_pct,
        active_repo: active_path,
        maximized,
        view,
    });
    let served = state.session.catalog().list_with_active(
        stored.active_repo.as_deref(),
        &stored.maximized,
        &stored.views,
    );
    let maximized: std::collections::HashMap<_, _> = served
        .maximized
        .into_iter()
        .map(|(id, panel)| (id, panel.as_str()))
        .collect();
    let last_view: std::collections::HashMap<_, _> = served
        .views
        .into_iter()
        .map(|(id, view)| (id, RepoViewDto::from(view)))
        .collect();
    super::encode_response(
        serde_json::json!({
            "accent": stored.accent,
            "sidebar_width": stored.sidebar_width,
            "upper_pct": stored.upper_pct,
            "active_repo": served.active,
            "maximized": maximized,
            "last_view": last_view,
        }),
        "could not encode preferences",
    )
}

/// Turn a client's view into the form the prefs file keeps: its repo id
/// becomes the path that file is keyed by, and the names it carries have to
/// be ones this build knows.
///
/// Paths are not checked here — they are checked where they are stored
/// (`prefs::repo_view`), the door the file itself also comes through; a
/// second check here would be a second place for the rule to drift.
fn resolve_view(request: ViewRequest, state: &ViewerState) -> Result<RepoView, Vec<u8>> {
    let Some(entry) = state.session.catalog().get(&request.repo) else {
        return Err(json_error("400 Bad Request", "unknown repo"));
    };
    let Some(tab) = ViewTab::parse(&request.tab) else {
        return Err(json_error("400 Bad Request", "unknown tab"));
    };
    let file = match request.file {
        Some(file) => {
            let Some(face) = ViewFace::parse(&file.face) else {
                return Err(json_error("400 Bad Request", "unknown file face"));
            };
            Some(ViewFile {
                path: file.path,
                commit: file.commit,
                face,
            })
        }
        None => None,
    };
    Ok(RepoView {
        repo: entry.path.clone(),
        tab,
        file,
        tree_expanded: request.tree_expanded,
    })
}
