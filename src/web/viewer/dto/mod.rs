//! The viewer's wire format.
//!
//! Internal git types are never serialized directly — they carry TUI-only
//! fields (`search_lower`, `summary_lower`) and libgit2-shaped types like
//! `Oid`. Every payload below is an explicit whitelist built by hand, so
//! adding a field to an internal struct can never widen what a browser sees,
//! and renaming one breaks the build here instead of silently changing the API.
//!
//! [`PROTOCOL_VERSION`] rides on every response so a cached page from an
//! older build can refuse to interpret a newer payload rather than misread
//! it.

mod diff;
mod envelope;
mod log;
mod status;
mod tree;

pub use diff::{DiffDto, FileDto, SpanDto};
#[cfg(test)]
pub use diff::{DiffHunkDto, DiffLineDto};
#[cfg(test)]
pub use envelope::PROTOCOL_VERSION;
pub use envelope::{Envelope, HotConfigDto, RepoDto, ViewerBootstrapDto};
#[cfg(test)]
pub use log::CommitDto;
pub use log::{CommitFilesDto, LogDto};
pub use status::{BrowseDto, BrowseEntryDto, StatusDto};
#[cfg(test)]
pub use status::{ChangedFileDto, TrackingDto, server_now_millis};
pub use tree::{TreeDto, TreeSearchDto};
#[cfg(test)]
pub use tree::{TreeEntryDto, TreeMatchDto};

#[cfg(test)]
mod tests;
