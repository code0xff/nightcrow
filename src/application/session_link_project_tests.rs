//! What a project chord actually asks the session for, against a real daemon.
//!
//! The resolvers in `session_link_tests` prove which id or which order a chord
//! means, and the daemon tests prove what the session does with a request that
//! arrives. Neither watches one leave: a `Move` arm that resolved an order and
//! never sent it, or sent it under the wrong direction, satisfies both sides and
//! still moves nothing. `DaemonClient::reorder_repos` has no other caller in the
//! tests, so this is where it is exercised.

use super::session_terminals_tests::attached;
use crate::application::input::dispatch::{ProjectContext, ProjectRequest};
use crate::application::session_link::SessionLink;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

/// Only the watcher's tick is waited on — a default project starts no terminal,
/// so no shell is in the way.
const DEADLINE: Duration = Duration::from_secs(5);

/// Run the main loop's tick — take in whatever the session said — until `done`,
/// and report whether it happened.
fn sync_until(
    link: &mut SessionLink,
    ws: &mut Workspace,
    ctx: &ProjectContext,
    mut done: impl FnMut(&Workspace) -> bool,
) -> bool {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        link.sync(ws, ctx);
        if done(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// The tab strip as the client is showing it, by repository path. Paths rather
/// than ids because the assertions are about slots, and the session's own
/// spelling of a path is whatever it broadcast.
fn strip(ws: &Workspace) -> Vec<String> {
    ws.projects()
        .iter()
        .map(|project| project.repository_path().to_string())
        .collect()
}

/// Both tabs arrived *and* carry the catalog id a request names them by — a tab
/// without one makes every project request an early-out.
fn both_tabs_named(ws: &Workspace) -> bool {
    ws.projects().len() == 2 && ws.projects().iter().all(|p| p.repository_id().is_some())
}

/// A client attached to a session serving two repositories, with its tabs
/// adopted and the first one in front.
struct Linked {
    _socket: crate::daemon::socket::DaemonSocket,
    _repos: (tempfile::TempDir, tempfile::TempDir),
    _dir: tempfile::TempDir,
    link: SessionLink,
    ws: Workspace,
}

fn linked(cfg: &crate::config::Config) -> (Linked, ProjectContext<'_>) {
    let (repo_a, path_a) = crate::test_util::make_repo();
    let (repo_b, path_b) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let (socket, client) = attached(&dir, &[path_a, path_b]);
    let leader = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
    let ctx = ProjectContext { cfg, leader };
    let mut linked = Linked {
        _socket: socket,
        _repos: (repo_a, repo_b),
        _dir: dir,
        link: SessionLink::new(client),
        ws: Workspace::new(leader),
    };
    assert!(
        sync_until(&mut linked.link, &mut linked.ws, &ctx, both_tabs_named),
        "the session's tabs never reached the client"
    );
    // Nothing has been focused, so the session names the repository it serves
    // first — which is the case the front-tab pinning is about.
    assert_eq!(linked.ws.active_index(), 0);
    (linked, ctx)
}

#[test]
fn moving_the_active_project_reorders_the_session_and_keeps_the_tab_in_front() {
    let cfg = crate::config::Config::default();
    let (mut it, ctx) = linked(&cfg);
    let before = strip(&it.ws);
    let swapped: Vec<String> = before.iter().rev().cloned().collect();

    it.link
        .request(&mut it.ws, ProjectRequest::Move { forward: true });

    assert!(
        sync_until(&mut it.link, &mut it.ws, &ctx, |ws| strip(ws) == swapped),
        "the strip stayed {:?}",
        strip(&it.ws)
    );
    // Reordering is not switching. The session tracks the front tab separately
    // from the order, so the request has to leave it with the repository it was
    // on — now in the other slot.
    assert_eq!(it.ws.projects()[1].repository_path(), before[0]);
    assert_eq!(
        it.ws.active_index(),
        1,
        "the front tab followed its repository, not its slot"
    );
}

#[test]
fn stepping_between_projects_puts_the_other_tab_in_front_and_leaves_the_strip() {
    let cfg = crate::config::Config::default();
    let (mut it, ctx) = linked(&cfg);
    let before = strip(&it.ws);

    it.link
        .request(&mut it.ws, ProjectRequest::Cycle { forward: true });

    assert!(
        sync_until(&mut it.link, &mut it.ws, &ctx, |ws| ws.active_index() == 1),
        "the front tab never moved"
    );
    assert_eq!(strip(&it.ws), before, "and stepping reordered nothing");
}
