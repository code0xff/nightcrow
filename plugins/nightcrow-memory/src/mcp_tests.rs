use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// Run the lines through a fresh server and return its replies, parsed.
fn exchange(db: Option<PathBuf>, token: &str, lines: &[String]) -> Vec<Value> {
    let open = move || match &db {
        Some(path) => Store::open(path),
        None => Err(anyhow::anyhow!("not started inside a nightcrow pane")),
    };
    let mut server = Server::new(open, || 5_000, Some(token.to_string()));
    let mut out = Vec::new();
    server.serve(lines.join("\n").as_bytes(), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn init(client: &str, version: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": version, "capabilities": {},
        "clientInfo": { "name": client, "version": "1" } } })
    .to_string()
}

fn call(id: i64, name: &str, args: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } })
    .to_string()
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap()
}

#[test]
fn initialize_echoes_a_supported_version_and_offers_tools() {
    let replies = exchange(None, "t", &[init("claude-code", "2025-06-18")]);
    let result = &replies[0]["result"];
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert!(result["capabilities"]["tools"].is_object());
    assert_eq!(result["serverInfo"]["name"], "nightcrow-memory");
}

#[test]
fn an_unknown_version_is_answered_with_the_newest_supported() {
    let replies = exchange(None, "t", &[init("x", "1999-01-01")]);
    assert_eq!(
        replies[0]["result"]["protocolVersion"],
        SUPPORTED_VERSIONS[0]
    );
}

#[test]
fn notifications_and_client_responses_get_no_reply() {
    let replies = exchange(
        None,
        "t",
        &[
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
            json!({ "jsonrpc": "2.0", "id": 9, "result": {} }).to_string(),
            String::new(),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }).to_string(),
        ],
    );
    assert_eq!(replies.len(), 1);
    assert_eq!(
        replies[0],
        json!({ "jsonrpc": "2.0", "id": 2, "result": {} })
    );
}

#[test]
fn tools_list_names_every_tool() {
    let replies = exchange(
        None,
        "t",
        &[json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" }).to_string()],
    );
    let names: Vec<_> = replies[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "memory_write",
            "memory_search",
            "memory_recent",
            "memory_delete",
            "board_post",
            "board_read"
        ]
    );
    assert_eq!(replies[0]["id"], "a");
}

#[test]
fn a_note_is_signed_with_the_client_name_and_the_pane() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("p.db");
    let replies = exchange(
        Some(db),
        "3fa9c1ffffffffff",
        &[
            init("claude-code", "2025-06-18"),
            call(2, "memory_write", json!({ "text": "tabs below md" })),
            call(3, "memory_recent", json!({})),
        ],
    );
    assert_eq!(replies[1]["result"]["isError"], false);
    assert!(
        text(&replies[2]).contains("#1 · claude-code@3fa9c1 · just now"),
        "{}",
        text(&replies[2])
    );
}

#[test]
fn a_failed_tool_is_a_result_the_agent_reads_not_a_protocol_error() {
    let dir = TempDir::new().unwrap();
    let replies = exchange(
        Some(dir.path().join("p.db")),
        "t",
        &[
            call(1, "memory_write", json!({ "text": "" })),
            call(2, "nope", json!({})),
        ],
    );
    for reply in &replies {
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["result"]["isError"], true);
    }
    assert!(text(&replies[0]).contains("empty"));
}

#[test]
fn outside_a_pane_the_server_answers_and_each_call_says_why_it_cannot_work() {
    let replies = exchange(
        None,
        "t",
        &[
            init("codex", "2025-06-18"),
            call(2, "memory_recent", json!({})),
        ],
    );
    assert!(replies[0]["result"].is_object());
    assert_eq!(replies[1]["result"]["isError"], true);
    assert!(text(&replies[1]).contains("not started inside"));
}

#[test]
fn malformed_requests_get_the_matching_json_rpc_error() {
    let long = "x".repeat(MAX_LINE_BYTES as usize + 10);
    let replies = exchange(
        None,
        "t",
        &[
            "{not json".to_string(),
            "[1,2]".to_string(),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }).to_string(),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {} }).to_string(),
            json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call",
                    "params": { "name": "board_read", "arguments": [] } })
            .to_string(),
            long,
            json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }).to_string(),
        ],
    );
    let codes: Vec<_> = replies
        .iter()
        .map(|r| r["error"]["code"].as_i64())
        .collect();
    assert_eq!(
        codes,
        [
            Some(PARSE_ERROR),
            Some(INVALID_REQUEST),
            Some(METHOD_NOT_FOUND),
            Some(INVALID_PARAMS),
            Some(INVALID_PARAMS),
            Some(PARSE_ERROR),
            None
        ]
    );
    // The line after the oversized one is still read on its own boundary.
    assert_eq!(replies[6]["id"], 7);
}
