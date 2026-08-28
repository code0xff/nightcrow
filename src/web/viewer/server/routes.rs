use super::ViewerState;
use super::handlers::{
    encode, open_repo, optional_count, optional_oid, required_oid, required_path, with_repo,
    with_repo_git_path,
};
use super::http_util::{json_error, json_response};
use super::mutations::redact;
use crate::git::diff;
use crate::web::common::http::RequestHead;
use crate::web::viewer::dto::{
    BrowseDto, BrowseEntryDto, CommitFilesDto, DiffDto, Envelope, FileDto, HotConfigDto, LogDto,
    StatusDto, TreeDto, TreeSearchDto, ViewerBootstrapDto,
};
use crate::web::viewer::limits;

pub(super) fn route(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    if head.method != "GET" {
        return json_error("405 Method Not Allowed", "only GET is supported");
    }
    match head.path.as_str() {
        "/api/repos" => {
            // Everything server-wide the client must agree with rides this one
            // response rather than getting endpoints of its own: the client
            // already polls it every few seconds, so a setting changed here
            // reaches every device within one interval.
            let prefs = state.session.prefs().get();
            // The remembered project is resolved to an id per response rather
            // than stored as one, and from the same snapshot as the list it
            // will be rendered against — see `Catalog::list_with_active`.
            let served = state.session.catalog().list_with_active(
                prefs.active_repo.as_deref(),
                &prefs.maximized,
                &prefs.views,
            );
            let bootstrap = ViewerBootstrapDto::new(
                served.list.into_iter().map(Into::into).collect(),
                HotConfigDto {
                    enabled: state.hot.enabled,
                    window_secs: state.hot.hot_window_secs,
                },
                &prefs,
                served.active,
                served
                    .maximized
                    .into_iter()
                    .map(|(id, panel)| (id, panel.as_str()))
                    .collect(),
                served
                    .views
                    .into_iter()
                    .map(|(id, view)| (id, view.into()))
                    .collect(),
                state.git_available,
            );
            match serde_json::to_string(&Envelope::new(bootstrap)) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
            }
        }
        "/api/status" => with_repo(head, state, |entry| {
            // Served from the runtime's latest snapshot rather than a fresh git
            // call while it is watching: the watch already reads the tree every
            // second, and this keeps a page refresh from queueing another walk.
            // With nothing subscribed the watch is off and `latest` is whatever
            // was true when the last client left — so this reads once instead.
            if !entry.runtime.is_watching() {
                entry.runtime.refresh_now();
            }
            match entry.runtime.latest() {
                Some(update) => Ok(json_response("200 OK", &update.json, &[])),
                None => Ok(json_response(
                    "200 OK",
                    &encode(&StatusDto::from_snapshot(
                        &[],
                        None,
                        None,
                        None,
                        &std::collections::HashMap::new(),
                    ))?,
                    &[],
                )),
            }
        }),
        "/api/tree" => with_repo(head, state, |entry| {
            let path = head.query_param("path").unwrap_or_default();
            let repo = open_repo(&entry.path)?;
            let workdir = repo
                .workdir()
                .ok_or_else(|| anyhow::anyhow!("bare repository"))?
                .to_path_buf();
            let entries = crate::git::tree::read_children(&repo, &workdir, &path, true)?;
            Ok(json_response(
                "200 OK",
                &encode(&TreeDto::from_entries(&path, &entries))?,
                &[],
            ))
        }),
        "/api/tree/search" => with_repo(head, state, |entry| {
            let query = head.query_param("q").unwrap_or_default();
            // An empty query would match every entry, and an over-long one is not
            // a real filename search; both short-circuit to an empty result so the
            // walk never runs on degenerate input.
            let (matches, truncated) =
                if query.is_empty() || query.len() > limits::MAX_TREE_SEARCH_QUERY_BYTES {
                    (Vec::new(), false)
                } else {
                    let repo = open_repo(&entry.path)?;
                    let workdir = repo
                        .workdir()
                        .ok_or_else(|| anyhow::anyhow!("bare repository"))?
                        .to_path_buf();
                    crate::git::tree::search_tree(
                        &repo,
                        &workdir,
                        &query,
                        limits::MAX_TREE_SEARCH_DEPTH,
                        limits::MAX_TREE_SEARCH_VISITS,
                        limits::MAX_TREE_SEARCH_RESULTS,
                    )?
                };
            Ok(json_response(
                "200 OK",
                &encode(&TreeSearchDto::new(&query, &matches, truncated))?,
                &[],
            ))
        }),
        // Through the git gate, not the worktree one: a file the working tree no
        // longer holds still has a diff — its deletion — and requiring it to be
        // on disk is what made a deleted path a 400 in a list that shows it.
        "/api/diff" => with_repo_git_path(head, state, |entry, path| {
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_file_diff(&repo, path)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(path, &hunks))?,
                &[],
            ))
        }),
        "/api/file" => with_repo(head, state, |entry| {
            let path = required_path(head)?;
            let repo = open_repo(&entry.path)?;
            let content = diff::load_workdir_file(&repo, &path)?;
            Ok(json_response(
                "200 OK",
                &encode(&FileDto::new(&path, &content))?,
                &[],
            ))
        }),
        // A repository file served as itself — the one exception to "the API
        // answers *about* the repository, never with its files". See `preview`
        // for what its response's own policy opens and keeps shut.
        "/api/preview" => super::preview::route(head, state),
        "/api/log" => with_repo(head, state, |entry| {
            let repo = open_repo(&entry.path)?;
            // `from` pins the walk so a page fetched later continues the
            // history the earlier pages described, even if commits landed
            // meanwhile — and a terminal that commits sits right below this
            // list. Resolved once, and the walk is then given exactly this
            // oid: asking the loader to fall back to HEAD itself would read
            // the ref a second time, and a first commit landing between the
            // two reads would return commits under an anchor of `None`,
            // which the client reads as the end of the history.
            let skip = optional_count(head, "skip")?;
            let anchor = match optional_oid(head, "from")? {
                Some(oid) => Some(oid),
                None => diff::head_commit_oid(&repo)?,
            };
            let commits = match anchor {
                // One more than a page, so a full page can be told apart from
                // a page that happens to end at the last commit.
                Some(oid) => {
                    diff::load_commit_log_from(&repo, Some(oid), skip, limits::MAX_LOG_PAGE + 1)?
                }
                // No commit to walk from: an unborn HEAD, which is a
                // repository with no history rather than an error.
                None => Vec::new(),
            };
            Ok(json_response(
                "200 OK",
                &encode(&LogDto::from_entries(&commits, anchor))?,
                &[],
            ))
        }),
        "/api/commit" => with_repo(head, state, |entry| {
            let oid = required_oid(head)?;
            let oid_text = oid.to_string();
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_commit_diff(&repo, oid)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(&oid_text, &hunks))?,
                &[],
            ))
        }),
        "/api/commit/files" => with_repo(head, state, |entry| {
            let oid = required_oid(head)?;
            let repo = open_repo(&entry.path)?;
            let files = diff::load_commit_files(&repo, oid)?;
            Ok(json_response(
                "200 OK",
                &encode(&CommitFilesDto::from_entries(&files))?,
                &[],
            ))
        }),
        "/api/commit/file-diff" => with_repo_git_path(head, state, |entry, path| {
            let oid = required_oid(head)?;
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_commit_file_diff(&repo, oid, path)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(path, &hunks))?,
                &[],
            ))
        }),
        "/api/commit/file" => with_repo_git_path(head, state, |entry, path| {
            let oid = required_oid(head)?;
            let repo = open_repo(&entry.path)?;
            let content = diff::load_commit_file(&repo, oid, path)?;
            Ok(json_response(
                "200 OK",
                &encode(&FileDto::new(path, &content))?,
                &[],
            ))
        }),
        "/api/browse" => browse(head),
        _ => json_error("404 Not Found", "no such route"),
    }
}

/// List the server sub-directories under `path` (home when absent) for the
/// folder picker. Directories only, hidden ones skipped. Deliberately
/// unconfined — the picker browses the server to find a repo to open — but
/// reachable only authenticated and at the same trust as the terminal.
fn browse(head: &RequestHead) -> Vec<u8> {
    let start = match head.query_param("path").filter(|p| !p.is_empty()) {
        Some(path) => std::path::PathBuf::from(path),
        None => dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")),
    };
    match list_directories(&start) {
        Ok(dto) => match serde_json::to_string(&Envelope::new(dto)) {
            Ok(json) => json_response("200 OK", &json, &[]),
            Err(_) => json_error("500 Internal Server Error", "could not encode listing"),
        },
        Err(err) => redact(&head.path, &err),
    }
}

fn list_directories(path: &std::path::Path) -> anyhow::Result<BrowseDto> {
    use anyhow::Context;
    let canonical = path
        .canonicalize()
        .with_context(|| "path could not be resolved")?;
    if !canonical.is_dir() {
        anyhow::bail!("not a directory");
    }
    let mut entries: Vec<BrowseEntryDto> = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(&canonical).with_context(|| "directory is not readable")? {
        let Ok(entry) = entry else { continue };
        // `file_type` does not follow symlinks, so a symlinked directory is
        // skipped rather than risking a browse loop.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if entries.len() >= limits::MAX_TREE_ENTRIES {
            truncated = true;
            break;
        }
        let is_repo = entry.path().join(".git").exists();
        entries.push(BrowseEntryDto { name, is_repo });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(BrowseDto {
        path: crate::platform::paths::for_display(&canonical).into_owned(),
        parent: canonical
            .parent()
            .map(|p| crate::platform::paths::for_display(p).into_owned()),
        entries,
        truncated,
    })
}
