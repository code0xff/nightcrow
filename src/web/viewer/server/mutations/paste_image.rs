//! Receiving an image pasted into a terminal pane.
//!
//! A pane is a PTY on this machine, so a program in it can only be handed an
//! image as a file it can open. The browser holding the clipboard may be on a
//! different machine entirely — that is the whole reason the CLI's own
//! clipboard paste cannot reach it — so the bytes come over the wire and are
//! written here, and the client types the resulting path into the pane.
//!
//! Nothing about the path comes from the caller. The name is random, the
//! extension is decided by sniffing the bytes, and the directory is fixed, so
//! this route cannot be steered at a path of the caller's choosing and needs
//! no worktree gate. Files land outside every repository on purpose: a pasted
//! screenshot is not a change to the project and must not show up in its
//! status.

use super::super::http_util::json_error;
use super::lookup::redact;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Where pasted images are written, under the directory that already holds the
/// daemon socket, session store and run state.
const PASTE_DIR: &str = "tmp";

/// How long a pasted image stays before a later paste sweeps it. A person
/// pastes an image to talk about it now; a day is well past that and still
/// survives a long session.
const PASTE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Files kept regardless of age, newest first. A bound on the directory for the
/// case the TTL cannot reach: many pastes inside one day.
const MAX_KEPT: usize = 200;

/// The image formats a browser puts on the clipboard, by the bytes that
/// identify them. Sniffed rather than taken from `Content-Type`, so the
/// extension always describes the file that was actually written.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

fn paste_dir() -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine the home directory"))?;
    Ok(home.join(".nightcrow").join(PASTE_DIR))
}

/// Drop images past the TTL, then the oldest of whatever is left over the cap.
///
/// Best-effort throughout: a file that cannot be read or removed is left
/// alone. Failing the paste over an old file nobody asked about would trade a
/// working feature for a tidy directory.
fn sweep(dir: &std::path::Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut kept: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        match now.duration_since(modified) {
            Ok(age) if age > PASTE_TTL => {
                let _ = std::fs::remove_file(&path);
            }
            _ => kept.push((modified, path)),
        }
    }
    if kept.len() <= MAX_KEPT {
        return;
    }
    kept.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in &kept[MAX_KEPT..] {
        let _ = std::fs::remove_file(path);
    }
}

fn random_stem() -> anyhow::Result<String> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("OS RNG unavailable for a pasted image name: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Sweep `dir`, then write `bytes` into it under a fresh random name.
///
/// Separate from the route so the write can be tested somewhere other than the
/// caller's own home directory.
fn write_paste(dir: &std::path::Path, bytes: &[u8], extension: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    crate::platform::fs::set_owner_only_dir(dir);
    sweep(dir, SystemTime::now());
    let target = dir.join(format!("paste-{}.{extension}", random_stem()?));
    std::fs::write(&target, bytes)?;
    crate::platform::fs::set_owner_only(&target);
    Ok(target)
}

/// Write a pasted image and answer with the path a pane can be told to open.
pub(in crate::web::viewer::server) fn handle_paste_image(bytes: &[u8]) -> Vec<u8> {
    let Some(extension) = image_extension(bytes) else {
        return json_error(
            "415 Unsupported Media Type",
            "the pasted data is not a PNG, JPEG, GIF or WebP image",
        );
    };
    let target = match paste_dir().and_then(|dir| write_paste(&dir, bytes, extension)) {
        Ok(target) => target,
        Err(err) => return redact("writing a pasted image", &err),
    };
    super::encode_response(
        serde_json::json!({
            "path": crate::platform::paths::for_display(&target).into_owned()
        }),
        "could not encode the pasted image path",
    )
}

#[cfg(test)]
#[path = "paste_image_tests.rs"]
mod tests;
