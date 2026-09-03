use super::super::{
    BrowseDto, BrowseEntryDto, ChangedFileDto, CommitDto, CommitFilesDto, DiffDto, DiffHunkDto,
    DiffLineDto, FileDto, HotConfigDto, LogDto, PROTOCOL_VERSION, RepoDto, RepoViewDto, SpanDto,
    StatusDto, TrackingDto, TreeDto, TreeEntryDto, TreeMatchDto, TreeSearchDto, ViewFileDto,
    ViewerBootstrapDto,
};
use crate::session::prefs::{MaximizedPanel, ViewFace, ViewTab};

/// Where the wire fixture lives. At the `viewer-ui` root rather than under
/// `viewer-ui/src` (which the published crate excludes) so it ships in the
/// package and this test still passes from an installed crate; the
/// TypeScript side reaches it with a `../` import. The fixture is only half
/// a contract test on its own, and the half that matters is the one that
/// fails when the two hand-written definitions of this protocol drift apart.
const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/viewer-ui/api.fixture.json");

/// Set to rewrite the fixture instead of asserting against it.
const UPDATE_ENV: &str = "UPDATE_API_FIXTURE";

/// One instance of every payload the viewer serves, with literal values
/// rather than values derived from a repository — the point is the shape,
/// and a fixture that changed with the fixture repo could not be diffed.
///
/// Optional fields appear both present and absent (a renamed file next to a
/// plain one, a commit hunk next to a single-file one) so a
/// `skip_serializing_if` that stops firing is visible here too.
fn wire_fixture() -> serde_json::Value {
    let span = |t: &str, c: &str| SpanDto {
        t: t.to_string(),
        c: c.to_string(),
    };
    let changed = ChangedFileDto {
        path: "src/main.rs".to_string(),
        old_path: None,
        index: "M".to_string(),
        worktree: " ".to_string(),
        mtime: Some(1_700_000_000_000),
    };
    let renamed = ChangedFileDto {
        path: "src/app/mod.rs".to_string(),
        old_path: Some("src/app.rs".to_string()),
        index: "R".to_string(),
        worktree: "M".to_string(),
        mtime: None,
    };

    serde_json::json!({
        "version": PROTOCOL_VERSION,
        "bootstrap": ViewerBootstrapDto {
            repos: vec![RepoDto {
                id: "r1".to_string(),
                name: "nightcrow".to_string(),
                display_path: "~/code/nightcrow".to_string(),
            }],
            hot: HotConfigDto { enabled: true, window_secs: 15 },
            accent: 2,
            upper_pct: 55,
            active_repo: Some("r1".to_string()),
            // Only the served projects appear, by id — a remembered one this
            // session is not serving has no id to name it by.
            //
            // Read off the enum rather than spelled out, here and below: the
            // client checks these strings against its own union, and a literal
            // would keep saying "terminal" after the variant behind it was
            // renamed — leaving both sides passing while the wire disagreed.
            maximized: std::collections::HashMap::from([(
                "r1".to_string(),
                MaximizedPanel::Terminal.as_str(),
            )]),
            // What the project was showing, by id and for the served set only,
            // for the same reasons. The three tabs and both faces are spread
            // across this and `storedPrefs` below, so the client's unions are
            // exercised whole.
            last_view: std::collections::HashMap::from([(
                "r1".to_string(),
                RepoViewDto {
                    tab: ViewTab::Status.as_str(),
                    file: Some(ViewFileDto {
                        path: "src/main.rs".to_string(),
                        // The working tree's copy: no commit to read it from.
                        commit: None,
                        face: ViewFace::Diff.as_str(),
                    }),
                    tree_expanded: Vec::new(),
                },
            )]),
            // Literal, not `server_now_millis()`: a fixture that moved every
            // run could not be committed.
            now_ms: 1_700_000_000_500,
            can_clone: true,
            // Literal for the same reason — this one moves with every rebuild
            // of the bundle, which is the whole point of it.
            viewer_build: Some("3f6a1c04".to_string()),
        },
        "status": StatusDto {
            branch: Some("dev".to_string()),
            head: Some("9a3bc2c".to_string()),
            tracking: Some(TrackingDto { ahead: 2, behind: 0 }),
            files: vec![changed.clone(), renamed.clone()],
            truncated: false,
        },
        "log": LogDto {
            commits: vec![CommitDto {
                oid: "9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string(),
                short_id: "9a3bc2c".to_string(),
                summary: "refactor: name the bootstrap payload".to_string(),
                author: "code0xff".to_string(),
                time: 1_700_000_000,
            }],
            // A page with more behind it, carrying the anchor the client
            // pins its next request to.
            truncated: true,
            head: Some("9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string()),
        },
        // A repository with no commits: no anchor to page from, which is
        // also how the client learns there is nothing more. Present so the
        // absent `head` is pinned as well as the populated one.
        "logEmpty": LogDto::from_entries(&[], None),
        "commitFiles": CommitFilesDto {
            files: vec![renamed],
            truncated: true,
        },
        "tree": TreeDto {
            path: "src".to_string(),
            entries: vec![
                TreeEntryDto { name: "web".to_string(), is_dir: true },
                TreeEntryDto { name: "main.rs".to_string(), is_dir: false },
            ],
            truncated: false,
        },
        "treeSearch": TreeSearchDto {
            query: "dto".to_string(),
            matches: vec![TreeMatchDto {
                path: "src/web/viewer/dto.rs".to_string(),
                is_dir: false,
            }],
            truncated: false,
        },
        "diff": DiffDto {
            path: "src/main.rs".to_string(),
            hunks: vec![
                DiffHunkDto {
                    header: "@@ -1,3 +1,4 @@".to_string(),
                    file_path: None,
                    lines: vec![
                        DiffLineDto {
                            kind: " ".to_string(),
                            spans: vec![span("fn main() {", "#c9d1d9")],
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                        },
                        DiffLineDto {
                            kind: "+".to_string(),
                            spans: vec![span("    ", ""), span("run()", "#79c0ff")],
                            old_lineno: None,
                            new_lineno: Some(2),
                        },
                    ],
                },
                DiffHunkDto {
                    header: "@@ -10,2 +10,2 @@".to_string(),
                    file_path: Some("src/lib.rs".to_string()),
                    lines: vec![DiffLineDto {
                        kind: "-".to_string(),
                        spans: vec![span("mod old;", "#ff7b72")],
                        old_lineno: Some(10),
                        new_lineno: None,
                    }],
                },
            ],
            truncated: false,
        },
        "file": FileDto {
            path: "README.md".to_string(),
            lines: vec![vec![span("# nightcrow", "#d2a8ff")], vec![]],
            truncated: false,
        },
        "browse": BrowseDto {
            path: "/Users/code0xff/code".to_string(),
            parent: Some("/Users/code0xff".to_string()),
            entries: vec![
                BrowseEntryDto { name: "nightcrow".to_string(), is_repo: true },
                BrowseEntryDto { name: "scratch".to_string(), is_repo: false },
            ],
            truncated: false,
        },
        "browseRoot": BrowseDto {
            path: "/".to_string(),
            parent: None,
            entries: vec![],
            truncated: false,
        },
        "openedRepo": serde_json::json!({ "repo": RepoDto {
            id: "r2".to_string(),
            name: "scratch".to_string(),
            display_path: "~/code/scratch".to_string(),
        }}),
        // What `POST /api/file` answers on a successful write: the blob oid of
        // the saved contents, which the client keeps as the base for its next
        // save so a stale write is caught. The `409` refusal it can also send
        // carries `currentHash` and is an error shape, not a served payload,
        // so it is not modelled here.
        "savedFile": serde_json::json!({
            "hash": "0123456789abcdef0123456789abcdef01234567",
        }),
        // The one-time token `POST /api/preview/edit` hands back for the frame
        // to load the assembled editable preview.
        "editPreview": serde_json::json!({ "token": "0123456789abcdef0123456789abcdef" }),
        // One shape for every `/api/prefs` write: the full stored prefs, so
        // a client that set the accent and one that set the width both read
        // the clamped result back the same way.
        "storedPrefs": serde_json::json!({
            "accent": 2,
            "upper_pct": 55,
            "active_repo": "r1",
            // Both panels, so the client's union is exercised whole: the
            // variants are only checked against the values that actually
            // appear here, and one of them alone would let the other be
            // renamed on this side without anything failing.
            "maximized": {
                "r1": MaximizedPanel::Terminal.as_str(),
                "r2": MaximizedPanel::Files.as_str(),
            },
            // The tabs and the face the bootstrap above does not carry, so
            // between them every variant appears somewhere.
            "last_view": {
                "r1": RepoViewDto {
                    tab: ViewTab::Tree.as_str(),
                    file: Some(ViewFileDto {
                        path: "src/ui/mod.rs".to_string(),
                        commit: None,
                        face: ViewFace::Source.as_str(),
                    }),
                    tree_expanded: vec!["src".to_string(), "src/ui".to_string()],
                },
                "r2": RepoViewDto {
                    tab: ViewTab::Log.as_str(),
                    // Read from a commit rather than the working tree, so the
                    // optional `commit` appears present as well as absent.
                    file: Some(ViewFileDto {
                        path: "src/app.rs".to_string(),
                        commit: Some("9a3bc2cf0e1d2a3b4c5d6e7f8a9b0c1d2e3f4a5b".to_string()),
                        face: ViewFace::Diff.as_str(),
                    }),
                    tree_expanded: Vec::new(),
                },
            },
        }),
        // What `/api/reload` answers. A sentence rather than counts, because it
        // is the whole of what the browser has to show: a reload changes nothing
        // on the page, so this text is the only evidence the button did anything.
        // Built by `reload::ReloadReport::summary`, so the TUI's notice and this
        // toast cannot drift apart.
        "reloaded": serde_json::json!({
            "summary": crate::session::reload::ReloadReport {
                plugins: 1,
                startup_commands: 2,
                auto_open: false,
                repos: 1,
                unreachable: 0,
            }
            .summary(),
        }),
    })
}

#[test]
fn the_wire_fixture_matches_the_served_payloads() {
    // The DTOs here and the interfaces in `viewer-ui/src/api.ts` describe
    // one protocol twice, by hand. This pins the Rust half: any rename,
    // removal, addition, or type change lands in the fixture diff, which is
    // what sends someone to `api.ts`. Regenerating then puts the other half
    // under a compiler that rejects a field the interfaces still name by its
    // old name or type — an added field passes, being one they simply do not
    // mention yet. See the header of `api.contract.test.ts`.
    let expected = format!(
        "{}\n",
        serde_json::to_string_pretty(&wire_fixture()).unwrap()
    );

    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::write(FIXTURE_PATH, &expected).expect("could not write the fixture");
        return;
    }

    let actual = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_default();
    assert_eq!(
        actual, expected,
        "the wire payloads no longer match {FIXTURE_PATH}. \
         Regenerate with `{UPDATE_ENV}=1 cargo test the_wire_fixture`, then update \
         viewer-ui/src/api.ts until `npm run build` passes again."
    );
}
