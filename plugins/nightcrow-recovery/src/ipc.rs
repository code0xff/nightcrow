//! The private socket a provider's helper processes report through.
//!
//! Claude Code invokes a hook and a statusline command as children of the
//! `claude` process, so they inherit the [`PANE_TOKEN_ENV`] value nightcrow
//! injected. Those children live for milliseconds and must not block their
//! parent: connect, write one line, exit. This module is that line's format
//! and both ends of the socket.
//!
//! Trust posture: anything that can reach the socket can claim to be any pane,
//! so the socket is created 0600 inside a 0700 directory and every field is
//! validated before it reaches the state machine. The token is a correlation
//! key, never an authorisation — the worst a forged message can do is make
//! this plugin ask the host for something, and the host judges that again.

use crate::protocol::PaneToken;
use crate::provider::{OutOfBand, SignalKind};
use crate::transport::{UnixListener, UnixStream};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// IPC message version, independent of the host protocol: this socket is between
/// two copies of this same binary, so a mismatch means a half-finished upgrade.
const IPC_VERSION: u32 = 1;

/// Directory and file name under the chosen runtime root.
const RUNTIME_DIR: &str = "nightcrow";
const SOCKET_FILE: &str = "recovery.sock";
/// Fallback root when `$XDG_RUNTIME_DIR` is unset (it usually is on macOS).
const HOME_RUNTIME_DIR: &str = ".nightcrow/run";

/// Only the owner may traverse the directory or speak to the socket. A pane
/// token is a correlation key and not a secret, but there is no reason to let
/// another local user inject one.
const DIR_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;

/// Restrict a path to its owner on platforms where that is meaningful.
/// On Windows the default ACL on a user-owned directory already suffices.
#[cfg(unix)]
fn restrict_to_owner(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot restrict {} to its owner", path.display()))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Longest IPC line accepted. Both senders forward a whitelist of small scalar
/// fields, so anything larger is a bug or an attempt to make this process
/// allocate.
pub const MAX_IPC_LINE_BYTES: usize = 8 * 1024;

/// Longest token accepted. The host mints 32 hex characters; the cap is double
/// that so a future widening does not need a change here.
const MAX_TOKEN_LEN: usize = 64;

/// How long either end will block on the socket. The sender runs inside a
/// provider's hook child, so it must give up quickly rather than hold up
/// someone's CLI; the receiver uses the same bound so one stalled client
/// cannot park the accept loop.
const IPC_TIMEOUT: Duration = Duration::from_millis(500);

/// One report from a provider helper process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcMessage {
    pub token: PaneToken,
    pub kind: SignalKind,
    pub payload: Value,
}

impl IpcMessage {
    pub fn into_signal(self) -> (PaneToken, OutOfBand) {
        (
            self.token,
            OutOfBand {
                kind: self.kind,
                payload: self.payload,
            },
        )
    }
}

/// Env var the host sets on a plugin process and on the panes of the same hub,
/// naming the directory they share. See nightcrow's `PLUGIN_RUNTIME_DIR_ENV`.
pub const RUNTIME_DIR_ENV: &str = "NIGHTCROW_PLUGIN_RUNTIME_DIR";

/// Where the socket lives.
///
/// The host's directory when it named one: a hub is per repository, so a
/// session with several projects runs several of this binary and one fixed
/// path would let only the first bind — a helper inside a pane would then
/// reach whichever instance won rather than the one watching it. Falling back
/// to the old fixed location keeps this runnable by hand and under a host too
/// old to say.
pub fn socket_path() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(RUNTIME_DIR_ENV).filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join(SOCKET_FILE));
    }
    socket_path_from(
        std::env::var_os("XDG_RUNTIME_DIR").as_deref(),
        dirs::home_dir().map(std::ffi::OsString::from).as_deref(),
    )
}

/// The path rule itself, separated from the environment so it can be tested
/// without mutating this process's variables.
fn socket_path_from(runtime_dir: Option<&OsStr>, home: Option<&OsStr>) -> Result<PathBuf> {
    if let Some(dir) = runtime_dir.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join(RUNTIME_DIR).join(SOCKET_FILE));
    }
    let home = home.filter(|h| !h.is_empty()).context(
        "neither XDG_RUNTIME_DIR nor a home directory is set, so there is nowhere to put the socket",
    )?;
    Ok(PathBuf::from(home).join(HOME_RUNTIME_DIR).join(SOCKET_FILE))
}

pub fn encode(msg: &IpcMessage) -> Result<String> {
    let line = serde_json::json!({
        "v": IPC_VERSION,
        "token": msg.token,
        "kind": msg.kind.as_wire(),
        "payload": msg.payload,
    })
    .to_string();
    ensure!(
        line.len() <= MAX_IPC_LINE_BYTES,
        "ipc message is {} bytes, over the {MAX_IPC_LINE_BYTES}-byte limit",
        line.len()
    );
    Ok(line)
}

/// Parse and fully validate one line from the socket.
///
/// Every failure names what was wrong: this is the boundary where untrusted
/// input becomes state, so a silently-coerced field would be the bug.
pub fn parse_line(line: &str) -> Result<IpcMessage> {
    ensure!(
        line.len() <= MAX_IPC_LINE_BYTES,
        "ipc line is {} bytes, over the {MAX_IPC_LINE_BYTES}-byte limit",
        line.len()
    );
    let value: Value =
        serde_json::from_str(line).map_err(|e| anyhow::anyhow!("ipc line is not JSON: {e}"))?;
    let object = value.as_object().context("ipc line is not a JSON object")?;
    let v = object
        .get("v")
        .and_then(Value::as_u64)
        .context("ipc line has no numeric \"v\"")?;
    ensure!(
        v == u64::from(IPC_VERSION),
        "ipc line claims version {v}, this build speaks {IPC_VERSION}"
    );
    let token = object
        .get("token")
        .and_then(Value::as_str)
        .context("ipc line has no string \"token\"")?;
    ensure!(!token.is_empty(), "ipc line carries an empty token");
    ensure!(
        token.len() <= MAX_TOKEN_LEN,
        "ipc token is {} characters, over the {MAX_TOKEN_LEN} limit",
        token.len()
    );
    ensure!(
        token.chars().all(|c| c.is_ascii_alphanumeric()),
        "ipc token holds characters a pane token cannot contain"
    );
    let kind_name = object
        .get("kind")
        .and_then(Value::as_str)
        .context("ipc line has no string \"kind\"")?;
    let kind = SignalKind::from_wire(kind_name)
        .with_context(|| format!("unknown ipc kind {kind_name:?}"))?;
    let payload = object
        .get("payload")
        .context("ipc line has no \"payload\"")?;
    ensure!(
        payload.is_object(),
        "ipc payload is not a JSON object, so there is nothing to read from it"
    );
    Ok(IpcMessage {
        token: token.to_string(),
        kind,
        payload: payload.clone(),
    })
}

/// Send one message and return. Used by the short-lived `hook` and `statusline`
/// modes, where a failure must be silent: the provider's helper process has no
/// business reporting our problems to its user.
pub fn send(path: &Path, msg: &IpcMessage) -> Result<()> {
    let line = encode(msg)?;
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("no recovery plugin listening at {}", path.display()))?;
    stream.set_write_timeout(Some(IPC_TIMEOUT))?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

/// The listening end. Unlinks its socket when dropped, so a normal exit leaves
/// nothing behind for the next run to trip over.
#[derive(Debug)]
pub struct Ipc {
    path: PathBuf,
    listener: UnixListener,
}

impl Ipc {
    pub fn bind(path: PathBuf) -> Result<Self> {
        let dir = path
            .parent()
            .context("socket path has no parent directory")?;
        fs::create_dir_all(dir)
            .with_context(|| format!("cannot create runtime directory {}", dir.display()))?;
        restrict_to_owner(dir, DIR_MODE)?;
        // A socket file left by a crashed run refuses bind with EADDRINUSE, and
        // there is no live listener behind it to protect.
        if path.exists() && UnixStream::connect(&path).is_err() {
            let _ = fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path).with_context(|| {
            let len = path.as_os_str().len();
            if len >= 108 {
                format!(
                    "cannot listen on {} — the path is {len} bytes, over the ~107 byte AF_UNIX limit",
                    path.display()
                )
            } else {
                format!("cannot listen on {}", path.display())
            }
        })?;
        restrict_to_owner(&path, SOCKET_MODE)?;
        Ok(Self { path, listener })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Start accepting in a background thread. One line per connection: a
    /// helper process has exactly one thing to say.
    ///
    /// `sink` returns `false` when the receiver is gone, which ends the thread.
    pub fn serve<S>(&self, sink: S) -> Result<()>
    where
        S: Fn(IpcMessage) -> bool + Send + 'static,
    {
        let listener = self
            .listener
            .try_clone()
            .context("cannot clone the ipc listener for its accept thread")?;
        std::thread::Builder::new()
            .name("recovery-ipc".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(IPC_TIMEOUT));
                    match read_one(stream) {
                        Ok(msg) => {
                            if !sink(msg) {
                                return; // the main loop is gone
                            }
                        }
                        // A malformed or truncated message is dropped: there is
                        // no channel back to a caller that has already exited.
                        Err(_) => continue,
                    }
                }
            })
            .context("cannot spawn the ipc accept thread")?;
        Ok(())
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn read_one(stream: UnixStream) -> Result<IpcMessage> {
    let mut reader = BufReader::new(stream).take(MAX_IPC_LINE_BYTES as u64 + 1);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        bail!("ipc connection closed without sending a line");
    }
    parse_line(line.trim_end_matches('\n'))
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod tests;
