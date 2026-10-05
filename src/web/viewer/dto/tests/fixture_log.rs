//! The log page and its decorations in the wire fixture, kept apart so the
//! fixture file stays within the line limit.

use super::super::{CommitDto, LogDecorationsDto, LogDto, RefDto};
use std::collections::BTreeMap;

pub(super) fn log_page() -> LogDto {
    LogDto {
        commits: vec![CommitDto {
            oid: "9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string(),
            short_id: "9a3bc2c".to_string(),
            summary: "refactor: name the bootstrap payload".to_string(),
            author: "code0xff".to_string(),
            time: 1_700_000_000,
            merge: true,
        }],
        // A page with more behind it, carrying the anchor the client
        // pins its next request to.
        truncated: true,
        head: Some("9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string()),
    }
}

/// One commit carrying a ref of each kind, one commit on each side of the
/// upstream — every variant the client renders.
pub(super) fn decorations() -> LogDecorationsDto {
    let tip = "9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string();
    LogDecorationsDto {
        refs: BTreeMap::from([(
            tip.clone(),
            vec![
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
        )]),
        ahead: vec![tip],
        behind: vec!["7c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d".to_string()],
    }
}
