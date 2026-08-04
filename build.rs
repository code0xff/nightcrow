//! Stamps the build's commit into `NIGHTCROW_COMMIT` so a running binary can
//! say which source it came from — the splash and the empty pane both show it.
//!
//! A missing or unreadable repository is not a build failure: the crate is also
//! built from a crates.io package and from `cargo install --git` checkouts, and
//! only the latter carries git metadata. Those builds report `unknown`.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rustc-env=NIGHTCROW_COMMIT={}", commit());
    watch_head();
}

fn commit() -> String {
    let Some(sha) = git(&["rev-parse", "--short=9", "HEAD"]) else {
        return "unknown".to_string();
    };
    // Only meaningful once the sha resolved: with no repository at all, the
    // diff below fails too and would read as "dirty".
    let dirty = Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .status()
        .is_ok_and(|status| !status.success());
    if dirty { format!("{sha}+") } else { sha }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Re-run when the checked-out commit moves. Both files are needed: `HEAD`
/// changes on a branch switch, the branch's ref file on a new commit.
///
/// Only existing paths are declared — cargo treats a missing one as changed and
/// would rebuild on every invocation.
fn watch_head() {
    let head = Path::new(".git/HEAD");
    if !head.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=.git/HEAD");

    let Ok(contents) = std::fs::read_to_string(head) else {
        return;
    };
    if let Some(reference) = contents.strip_prefix("ref: ") {
        let path = Path::new(".git").join(reference.trim());
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // A packed ref has no loose file; the pack itself is then the thing to watch.
    if Path::new(".git/packed-refs").exists() {
        println!("cargo:rerun-if-changed=.git/packed-refs");
    }
}
