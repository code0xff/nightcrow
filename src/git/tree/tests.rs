use super::*;
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path as StdPath;

mod listing;
mod path_security;
mod search;

fn names(entries: &[TreeEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn paths(matches: &[TreeMatch]) -> Vec<&str> {
    matches
        .iter()
        .map(|matched| matched.path.as_str())
        .collect()
}
