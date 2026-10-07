//! The program as an agent's CLI runs it: a child process spoken to over its
//! stdio, finding its project through the environment alone. Two of them here,
//! because the feature is one agent reading what another wrote.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

const PROJECT: &str = "/run/user/1000/nightcrow/00ab34cd56ef7890";
const OTHER_PROJECT: &str = "/run/user/1000/nightcrow/ffffffffffffffff";

struct Agent {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Agent {
    /// Start `nightcrow-memory mcp` as the pane `token` of `project` would, and
    /// complete the handshake as `client`.
    fn start(store_dir: &Path, project: Option<&str>, token: &str, client: &str) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nightcrow-memory"));
        command
            .arg("mcp")
            .env_remove("NIGHTCROW_PLUGIN_RUNTIME_DIR")
            .env("NIGHTCROW_MEMORY_DIR", store_dir)
            .env("NIGHTCROW_PANE_TOKEN", token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(project) = project {
            command.env("NIGHTCROW_PLUGIN_RUNTIME_DIR", project);
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut agent = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        let hello = agent.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": client, "version": "0" } }),
        );
        assert_eq!(hello["result"]["serverInfo"]["name"], "nightcrow-memory");
        agent.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        agent
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], id, "{reply}");
        reply
    }

    /// Call a tool; the text it answered with and whether it was an error.
    fn tool(&mut self, name: &str, args: Value) -> (String, bool) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }

    /// Close stdin, as a CLI that is done does, and expect a clean exit.
    fn finish(mut self) {
        drop(self.stdin);
        assert!(self.child.wait().unwrap().success());
    }
}

#[test]
fn an_agent_in_another_pane_of_the_project_reads_what_the_first_wrote() {
    let dir = TempDir::new().unwrap();
    let mut claude = Agent::start(dir.path(), Some(PROJECT), "aaaaaa1111", "claude-code");
    let mut codex = Agent::start(dir.path(), Some(PROJECT), "bbbbbb2222", "codex");

    let (saved, failed) = claude.tool(
        "memory_write",
        json!({ "text": "viewer-ui는 pnpm이 아니라 npm을 쓴다", "tags": ["build"] }),
    );
    assert!(!failed, "{saved}");
    claude.tool("board_post", json!({ "text": "editing the header" }));

    let (found, failed) = codex.tool("memory_search", json!({ "query": "npm" }));
    assert!(!failed, "{found}");
    assert!(found.contains("claude-code@aaaaaa"), "{found}");
    assert!(
        found.contains("viewer-ui는 pnpm이 아니라 npm을 쓴다"),
        "{found}"
    );

    let (board, _) = codex.tool("board_read", json!({}));
    assert!(
        board.contains("claude-code@aaaaaa") && board.contains("editing the header"),
        "{board}"
    );

    claude.finish();
    codex.finish();
}

#[test]
fn a_note_outlives_the_process_that_wrote_it() {
    let dir = TempDir::new().unwrap();
    let mut first = Agent::start(dir.path(), Some(PROJECT), "aaaaaa", "claude-code");
    first.tool("memory_write", json!({ "text": "kept across sessions" }));
    first.finish();

    let mut later = Agent::start(dir.path(), Some(PROJECT), "cccccc", "opencode");
    let (recent, _) = later.tool("memory_recent", json!({}));
    assert!(recent.contains("kept across sessions"), "{recent}");
    later.finish();
}

#[test]
fn another_project_sees_none_of_it() {
    let dir = TempDir::new().unwrap();
    let mut here = Agent::start(dir.path(), Some(PROJECT), "aaaaaa", "claude-code");
    let mut elsewhere = Agent::start(dir.path(), Some(OTHER_PROJECT), "dddddd", "claude-code");
    here.tool(
        "memory_write",
        json!({ "text": "private to this repository" }),
    );
    here.tool("board_post", json!({ "text": "busy here" }));

    let (recent, _) = elsewhere.tool("memory_recent", json!({}));
    assert!(!recent.contains("private to this repository"), "{recent}");
    let (board, _) = elsewhere.tool("board_read", json!({}));
    assert!(!board.contains("busy here"), "{board}");
    here.finish();
    elsewhere.finish();
}

#[test]
fn outside_a_nightcrow_pane_the_tools_explain_themselves_and_store_nothing() {
    let dir = TempDir::new().unwrap();
    let mut stray = Agent::start(dir.path(), None, "", "claude-code");
    let (text, failed) = stray.tool("memory_write", json!({ "text": "nowhere to go" }));
    assert!(failed);
    assert!(text.contains("NIGHTCROW_PLUGIN_RUNTIME_DIR"), "{text}");
    stray.finish();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn concurrent_writers_all_land() {
    let dir = TempDir::new().unwrap();
    let writers: Vec<_> = (0..4)
        .map(|n| {
            let store_dir = dir.path().to_path_buf();
            std::thread::spawn(move || {
                let mut agent = Agent::start(
                    &store_dir,
                    Some(PROJECT),
                    &format!("{n}{n}{n}{n}{n}{n}"),
                    "w",
                );
                for i in 0..10 {
                    let (text, failed) =
                        agent.tool("memory_write", json!({ "text": format!("w{n} n{i}") }));
                    assert!(!failed, "{text}");
                }
                agent.finish();
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    let mut reader = Agent::start(dir.path(), Some(PROJECT), "eeeeee", "r");
    let (all, _) = reader.tool("memory_recent", json!({ "limit": 50 }));
    assert_eq!(all.matches("\n#").count(), 40, "{all}");
    reader.finish();
}

#[test]
fn export_prints_the_project_as_markdown() {
    let dir = TempDir::new().unwrap();
    let mut agent = Agent::start(dir.path(), Some(PROJECT), "aaaaaa", "claude-code");
    agent.tool("memory_write", json!({ "text": "exported fact" }));
    agent.finish();

    let out = Command::new(env!("CARGO_BIN_EXE_nightcrow-memory"))
        .arg("export")
        .env("NIGHTCROW_MEMORY_DIR", dir.path())
        .env("NIGHTCROW_PLUGIN_RUNTIME_DIR", PROJECT)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("## Notes (1)") && text.contains("exported fact"),
        "{text}"
    );
}

#[test]
fn an_unknown_command_fails_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_nightcrow-memory"))
        .arg("serve")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: nightcrow-memory"));
}
