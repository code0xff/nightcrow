use super::*;
use serde_json::json;
use tempfile::TempDir;

fn store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("p.db")).unwrap();
    (dir, store)
}

const A: Caller<'static> = Caller {
    author: "claude-code@aaaaaa",
    now: 1_000,
};
const B: Caller<'static> = Caller {
    author: "codex@bbbbbb",
    now: 1_120,
};

#[test]
fn every_tool_listed_can_be_called() {
    let (_dir, mut store) = store();
    for tool in definitions().as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        // Wrong arguments are fine here: the point is that the name dispatches.
        let error = call(&mut store, &A, name, &json!({}))
            .err()
            .map(|e| e.to_string());
        assert!(
            !error
                .as_deref()
                .unwrap_or_default()
                .contains("unknown tool"),
            "{name} is listed but not handled"
        );
        assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
    }
}

#[test]
fn one_agent_finds_what_another_wrote_with_its_author() {
    let (_dir, mut store) = store();
    let saved = call(
        &mut store,
        &A,
        "memory_write",
        &json!({ "text": "the bundle is rebuilt per commit", "tags": ["viewer"] }),
    )
    .unwrap();
    assert_eq!(saved, "Saved as note #1.");

    let found = call(
        &mut store,
        &B,
        "memory_search",
        &json!({ "query": "bundle" }),
    )
    .unwrap();
    assert!(found.starts_with(render::PROVENANCE_NOTICE));
    assert!(
        found.contains("#1 · claude-code@aaaaaa · 2m ago [viewer]"),
        "{found}"
    );
    assert!(found.contains("the bundle is rebuilt per commit"));
}

#[test]
fn recent_and_search_say_so_when_there_is_nothing() {
    let (_dir, mut store) = store();
    assert!(
        call(&mut store, &A, "memory_recent", &json!({}))
            .unwrap()
            .contains("No notes")
    );
    let none = call(&mut store, &A, "memory_search", &json!({ "query": "x" })).unwrap();
    assert_eq!(none, "No notes match.");
}

#[test]
fn delete_removes_the_note_and_a_missing_id_is_an_error() {
    let (_dir, mut store) = store();
    call(&mut store, &A, "memory_write", &json!({ "text": "wrong" })).unwrap();
    assert_eq!(
        call(&mut store, &B, "memory_delete", &json!({ "id": 1 })).unwrap(),
        "Deleted note #1."
    );
    let error = call(&mut store, &B, "memory_delete", &json!({ "id": 1 })).unwrap_err();
    assert!(error.to_string().contains("no note #1"));
}

#[test]
fn the_board_shows_each_agents_latest_post() {
    let (_dir, mut store) = store();
    call(
        &mut store,
        &A,
        "board_post",
        &json!({ "text": "editing the parser" }),
    )
    .unwrap();
    call(
        &mut store,
        &B,
        "board_post",
        &json!({ "text": "writing docs", "ttl_minutes": 30 }),
    )
    .unwrap();
    let board = call(&mut store, &B, "board_read", &json!({})).unwrap();
    assert!(
        board.contains("claude-code@aaaaaa · 2m ago\n> editing the parser"),
        "{board}"
    );
    assert!(
        board.contains("codex@bbbbbb · just now\n> writing docs"),
        "{board}"
    );
}

#[test]
fn a_board_post_expires_after_its_minutes() {
    let (_dir, mut store) = store();
    call(
        &mut store,
        &A,
        "board_post",
        &json!({ "text": "brief", "ttl_minutes": 1 }),
    )
    .unwrap();
    let later = Caller {
        author: B.author,
        now: A.now + 60,
    };
    assert!(
        call(&mut store, &later, "board_read", &json!({}))
            .unwrap()
            .contains("Nobody")
    );
}

#[test]
fn missing_and_mistyped_arguments_are_named_in_the_error() {
    let (_dir, mut store) = store();
    for (tool, args, needle) in [
        ("memory_write", json!({}), "`text` is required"),
        (
            "memory_write",
            json!({ "text": 5 }),
            "`text` must be a string",
        ),
        (
            "memory_write",
            json!({ "text": "x", "tags": "one" }),
            "`tags` must be a list",
        ),
        (
            "memory_write",
            json!({ "text": "x", "tags": [1] }),
            "every tag must be a string",
        ),
        (
            "memory_search",
            json!({ "query": "x", "limit": 0 }),
            "at least 1",
        ),
        (
            "memory_search",
            json!({ "query": "x", "limit": "3" }),
            "whole number",
        ),
        ("memory_delete", json!({}), "`id` is required"),
        ("memory_delete", json!({ "id": 1.5 }), "whole number"),
        (
            "board_post",
            json!({ "text": "x", "ttl_minutes": 0 }),
            "lasts between",
        ),
        ("nope", json!({}), "unknown tool"),
    ] {
        let error = call(&mut store, &A, tool, &args).unwrap_err().to_string();
        assert!(error.contains(needle), "{tool} {args}: {error}");
    }
}

#[test]
fn an_absurd_ttl_is_refused_rather_than_overflowing() {
    let (_dir, mut store) = store();
    let args = json!({ "text": "x", "ttl_minutes": i64::MAX });
    assert!(call(&mut store, &A, "board_post", &args).is_err());
}
