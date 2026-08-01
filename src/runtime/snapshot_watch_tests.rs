use super::{Roots, any_matters, changed_paths, external_git_dir};
use crate::test_util::{make_linked_worktree, make_repo};
use notify::event::{AccessKind, AccessMode, CreateKind, Event, EventKind, Flag, ModifyKind};
use std::path::{Path, PathBuf};

#[path = "snapshot_watch_tests/events.rs"]
mod events;
#[path = "snapshot_watch_tests/filter.rs"]
mod filter;
#[path = "snapshot_watch_tests/roots.rs"]
mod roots;

fn under(root: &str, relative: &str) -> Vec<PathBuf> {
    vec![Path::new(root).join(relative)]
}

fn wakes_the_reader(root: &str, event: notify::Result<Event>) -> bool {
    let repo = crate::test_util::open_repo(root);
    changed_paths(event)
        .is_some_and(|paths| any_matters(Some(&repo), &Roots::of(Path::new(root)), &paths))
}

fn at(kind: EventKind, root: &str, relative: &str) -> notify::Result<Event> {
    Ok(Event::new(kind).add_path(Path::new(root).join(relative)))
}
