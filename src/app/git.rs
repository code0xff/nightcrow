mod auto_follow;
mod commit_log_apply;
mod commit_log_fetch;
mod commit_log_pagination;
pub(super) mod diff_load;
mod file_view_load;
mod git_view_manager;
mod load_apply;
mod load_controller;
mod repository_view;
mod snapshot_io;
mod tree;

pub use git_view_manager::GitViewManager;
#[cfg(test)]
pub use repository_view::RepositoryView;
