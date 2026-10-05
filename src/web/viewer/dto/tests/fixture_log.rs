//! The log page in the wire fixture, kept apart so the fixture file stays
//! within the line limit. One commit carrying every decoration the client
//! renders: a ref of each kind, divergence, and the merge flag.

use super::super::{CommitDto, LogDto, RefDto};

pub(super) fn log_page() -> LogDto {
    LogDto {
        commits: vec![CommitDto {
            oid: "9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string(),
            short_id: "9a3bc2c".to_string(),
            summary: "refactor: name the bootstrap payload".to_string(),
            author: "code0xff".to_string(),
            time: 1_700_000_000,
            refs: vec![
                RefDto {
                    kind: "head",
                    name: "dev".to_string(),
                },
                RefDto {
                    kind: "local",
                    name: "feat/graph".to_string(),
                },
                RefDto {
                    kind: "tag",
                    name: "v0.1.15".to_string(),
                },
                RefDto {
                    kind: "remote",
                    name: "origin/dev".to_string(),
                },
            ],
            divergence: Some("ahead"),
            merge: true,
        }],
        // A page with more behind it, carrying the anchor the client
        // pins its next request to.
        truncated: true,
        head: Some("9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string()),
    }
}
