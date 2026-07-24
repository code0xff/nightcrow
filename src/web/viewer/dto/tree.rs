use crate::web::viewer::limits::{self, Capped};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TreeEntryDto {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeDto {
    pub path: String,
    pub entries: Vec<TreeEntryDto>,
    pub truncated: bool,
}

impl TreeDto {
    pub fn from_entries(path: &str, entries: &[crate::git::tree::TreeEntry]) -> Self {
        let capped = Capped::new(entries.to_vec(), limits::MAX_TREE_ENTRIES);
        Self {
            path: path.to_string(),
            entries: capped
                .items
                .iter()
                .map(|e| TreeEntryDto {
                    name: e.name.clone(),
                    is_dir: e.is_dir,
                })
                .collect(),
            truncated: capped.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TreeMatchDto {
    pub path: String,
    pub is_dir: bool,
}

/// Result of a recursive `/api/tree/search`: full paths whose basename matched
/// the query, already sorted and capped by the search walk.
#[derive(Debug, Clone, Serialize)]
pub struct TreeSearchDto {
    pub query: String,
    pub matches: Vec<TreeMatchDto>,
    pub truncated: bool,
}

impl TreeSearchDto {
    pub fn new(query: &str, matches: &[crate::git::tree::TreeMatch], truncated: bool) -> Self {
        Self {
            query: query.to_string(),
            matches: matches
                .iter()
                .map(|m| TreeMatchDto {
                    path: m.path.clone(),
                    is_dir: m.is_dir,
                })
                .collect(),
            truncated,
        }
    }
}