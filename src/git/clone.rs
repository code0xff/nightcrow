//! Clone a remote repository by handing the URL to the `git` binary.
//!
//! libgit2 is not used here. The vendored build carries no SSH transport
//! (`libgit2-sys` pulls no `libssh2-sys`), so `git@host:path` — the form most
//! remotes are written in — would not resolve at all, and libgit2 also knows
//! nothing of credential helpers, `insteadOf` rewrites, or an agent-held key.
//! Delegating to `git` inherits that whole stack. This is not the "parse git's
//! output" pattern the project avoids: nothing here reads stdout, only the exit
//! status and stderr-on-failure.

use std::path::Path;
use std::process::Command;

/// URL schemes the clone form accepts.
///
/// This list is a security boundary, not a convenience. git resolves
/// `ext::<command>` by **executing that command**, so an unfiltered URL is
/// remote code execution — and passing the URL as an argv item behind `--`
/// does not help, because the scheme is interpreted after argument parsing.
/// `file://` and bare local paths are excluded too: the caller reaches local
/// directories through the folder picker, so accepting them here would only
/// widen what a URL can name.
const ALLOWED_SCHEMES: [&str; 5] = ["https://", "http://", "ssh://", "git://", "git+ssh://"];

/// Longest URL accepted, to keep a hostile client from parking megabytes in a
/// request body that is only ever a remote address.
pub const MAX_CLONE_URL_BYTES: usize = 2048;

#[derive(Debug, PartialEq, Eq)]
pub enum CloneUrlError {
    Empty,
    TooLong,
    /// Control characters, including NUL and newlines.
    Control,
    /// A scheme outside [`ALLOWED_SCHEMES`], most importantly `ext::`.
    Scheme,
    /// Accepted shape, but no repository name could be derived from it.
    NoName,
}

impl CloneUrlError {
    /// Client-facing text. Deliberately says what is wrong without echoing the
    /// input back into the response.
    pub fn message(&self) -> &'static str {
        match self {
            CloneUrlError::Empty => "a repository URL is required",
            CloneUrlError::TooLong => "that URL is too long",
            CloneUrlError::Control => "that URL contains invalid characters",
            CloneUrlError::Scheme => {
                "only https, http, ssh, and git URLs (or user@host:path) can be cloned"
            }
            CloneUrlError::NoName => "could not read a repository name from that URL",
        }
    }
}

/// Accept `url` as a remote address and return the directory name a clone of it
/// would create — the same name `git clone <url>` picks.
///
/// Accepted shapes are the [`ALLOWED_SCHEMES`] and scp-like `user@host:path`.
/// Everything else is rejected, so `ext::`, `file://`, and bare paths never
/// reach `git`.
pub fn validate_clone_url(url: &str) -> Result<String, CloneUrlError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(CloneUrlError::Empty);
    }
    if url.len() > MAX_CLONE_URL_BYTES {
        return Err(CloneUrlError::TooLong);
    }
    // Rejected before the scheme check: a newline could otherwise split the URL
    // into something a later reader treats as two lines, and NUL truncates.
    if url.chars().any(|c| c.is_control()) {
        return Err(CloneUrlError::Control);
    }
    if !has_allowed_scheme(url) {
        return Err(CloneUrlError::Scheme);
    }
    repo_name_from_url(url).ok_or(CloneUrlError::NoName)
}

/// A `://` anywhere means the URL is claiming a scheme, so it must be one of
/// the allowed ones — matched as a prefix, which is what keeps `ext::https://…`
/// from passing by merely containing an allowed scheme. Without `://` the only
/// remaining accepted shape is scp-like.
fn has_allowed_scheme(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("://") {
        return ALLOWED_SCHEMES
            .iter()
            .any(|scheme| url.len() > scheme.len() && lower.starts_with(scheme));
    }
    is_scp_like(url)
}

/// scp-like remotes (`user@host:path`, `host:path`) carry no scheme, so they
/// are recognised by shape: a colon that comes before any slash, with a
/// non-empty host and path around it. Requiring the colon to precede the first
/// slash is what keeps `./weird:name` and other local paths out.
fn is_scp_like(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let (host, path) = url.split_at(colon);
    let path = &path[1..];
    if host.is_empty() || path.is_empty() {
        return false;
    }
    if host.contains('/') {
        return false;
    }
    // `ext::sh -c ...` also has a colon before any slash; a scheme is only a
    // host when it does not look like one. Anything ending in `:` + `:` (the
    // `scheme::` form git uses for transport helpers) is out.
    !path.starts_with(':') && !host.contains('\\')
}

/// The directory `git clone` would create: the last non-empty path segment with
/// a trailing `.git` removed. Returns `None` when nothing usable is left, or
/// when the result would not be a plain single segment (`.`, `..`, hidden).
fn repo_name_from_url(url: &str) -> Option<String> {
    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    // Drop the authority so a URL that names only a host (`https://example.com`)
    // cannot pass the host off as a repository name.
    let path = match without_query.find("://") {
        Some(scheme_end) => {
            let after_scheme = &without_query[scheme_end + 3..];
            let host_end = after_scheme.find('/')?;
            &after_scheme[host_end + 1..]
        }
        None => &without_query[without_query.find(':')? + 1..],
    };
    let last = path
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || name.starts_with('.') || name.contains(std::path::MAIN_SEPARATOR) {
        return None;
    }
    Some(name.to_string())
}

/// Whether a usable `git` is on PATH.
///
/// Probed once at startup and reported to clients so the clone form can say up
/// front that it is unavailable, rather than accepting a URL and failing the
/// job. A `git` installed while the server runs is therefore not picked up
/// until a restart — the trade for not shelling out on every page load.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Most stderr kept from a failing clone. A remote controls this stream —
/// `remote:` sidebands are printed verbatim — so it cannot be collected
/// unbounded. Only the tail is wanted anyway: git's last line is the reason.
const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Read `reader` to EOF, keeping only the last [`MAX_STDERR_BYTES`].
///
/// Draining to the end matters as much as the cap: stopping early would fill
/// the pipe and block the child forever.
fn tail_of<R: std::io::Read>(mut reader: R) -> String {
    let mut kept: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                kept.extend_from_slice(&chunk[..n]);
                if kept.len() > MAX_STDERR_BYTES {
                    kept.drain(..kept.len() - MAX_STDERR_BYTES);
                }
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// Run `git clone -- <url> <dest>` to completion.
///
/// The URL is an argv item behind `--`, never a shell word, so no quoting or
/// escaping question arises; [`validate_clone_url`] has already ruled out the
/// schemes that would make argv placement insufficient. On failure the error
/// carries git's stderr, which is what tells the user "repository not found"
/// or "permission denied".
pub fn run_clone(url: &str, dest: &Path) -> anyhow::Result<()> {
    let mut child = Command::new("git")
        // Without this a remote that wants credentials makes git open
        // /dev/tty and wait for a human who is not there — the clone would
        // hang forever instead of reporting that it needs authentication.
        .env("GIT_TERMINAL_PROMPT", "0")
        // The ssh transport needs its own liveness settings: a connect that
        // never completes, or a session that stops answering, would otherwise
        // hold the clone open indefinitely. Neither bound touches a slow but
        // progressing transfer.
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o ConnectTimeout=30 -o ServerAliveInterval=30 -o ServerAliveCountMax=4",
        )
        // Abort a transfer that has stalled rather than one that is merely
        // slow. A wall-clock timeout cannot tell those apart — a large
        // repository legitimately takes many minutes — but a rate floor can:
        // under 1 KiB/s for 60 s is a dead connection, not a big clone.
        .arg("-c")
        .arg("http.lowSpeedLimit=1024")
        .arg("-c")
        .arg("http.lowSpeedTime=60")
        .arg("clone")
        .arg("--")
        .arg(url)
        .arg(dest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => {
                anyhow::anyhow!("git is not installed or not on PATH")
            }
            _ => anyhow::anyhow!("could not run git: {err}"),
        })?;
    // Read before waiting: a child that fills the pipe blocks until it is
    // drained, so waiting first could deadlock.
    let stderr = child.stderr.take().map(tail_of).unwrap_or_default();
    let status = child
        .wait()
        .map_err(|err| anyhow::anyhow!("could not wait for git: {err}"))?;
    if status.success() {
        return Ok(());
    }
    let detail = stderr.trim();
    if detail.is_empty() {
        anyhow::bail!("git clone failed");
    }
    // git's own last line is the actionable part; earlier lines are progress.
    let last = detail.lines().next_back().unwrap_or(detail);
    anyhow::bail!("{last}")
}

#[cfg(test)]
mod tests;
