use super::super::ViewerState;
use super::super::http_util::json_error;
use crate::session::prefs::{MaximizedPanel, MaximizedUpdate, PrefsUpdate};

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
}

#[derive(serde::Deserialize)]
struct MaximizedRequest {
    /// Repo id, translated to a path before it is stored.
    repo: String,
    /// `"files"`, `"terminal"`, or absent/null for nothing maximized.
    panel: Option<String>,
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

    let stored = state.session.prefs().update(PrefsUpdate {
        accent: request.accent,
        sidebar_width: request.sidebar_width,
        upper_pct: request.upper_pct,
        active_repo: active_path,
        maximized,
    });
    let served = state
        .session
        .catalog()
        .list_with_active(stored.active_repo.as_deref(), &stored.maximized);
    let maximized: std::collections::HashMap<_, _> = served
        .maximized
        .into_iter()
        .map(|(id, panel)| (id, panel.as_str()))
        .collect();
    super::encode_response(
        serde_json::json!({
            "accent": stored.accent,
            "sidebar_width": stored.sidebar_width,
            "upper_pct": stored.upper_pct,
            "active_repo": served.active,
            "maximized": maximized,
        }),
        "could not encode preferences",
    )
}
