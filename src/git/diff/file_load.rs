//! Reading a file's whole contents — from the working tree, or from a commit.
//!
//! Separate from `diff_load.rs`, which is about what changed. These answer the
//! other question a person asks of the same path: not "what moved" but "what
//! does it say", which is what the viewer switches to from a diff.

use super::types::StatusKind;
use anyhow::{Context, Result};
use git2::{Oid, Repository};

pub const MAX_FILE_VIEW_BYTES: usize = 5 * 1024 * 1024;

fn decode_file_view(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_FILE_VIEW_BYTES {
        return Err(anyhow::anyhow!(
            "file too large to preview: {} bytes",
            bytes.len()
        ));
    }
    std::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|_| anyhow::anyhow!("binary or non-utf8 file"))
}

pub fn load_workdir_file(repo: &Repository, file_path: &str) -> Result<String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repository"))?;
    let full = crate::git::path::resolve_in_workdir(workdir, file_path)?;
    // Size-check through the open handle rather than a second path lookup, so
    // the file that gets read is the one that was measured.
    let file = std::fs::File::open(&full).with_context(|| format!("failed to open {file_path}"))?;
    let len = file
        .metadata()
        .with_context(|| format!("failed to stat {file_path}"))?
        .len();
    // Reject a multi-GB log file or build artifact before it ever materializes
    // into memory: `decode_file_view`'s post-read length check would otherwise
    // allocate the full buffer before bailing.
    if len > MAX_FILE_VIEW_BYTES as u64 {
        return Err(anyhow::anyhow!("file too large to preview: {len} bytes"));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    {
        use std::io::Read;
        // Cap the read itself: `len` came from the handle, but a file that
        // grows between the stat and the read would otherwise be read in full.
        file.take(MAX_FILE_VIEW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read {file_path}"))?;
    }
    decode_file_view(&bytes)
}

pub fn load_commit_file_blob(
    repo: &Repository,
    oid: Oid,
    file_path: &str,
    status: StatusKind,
) -> Result<String> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let tree = if status == StatusKind::Deleted {
        commit
            .parent(0)
            .context("deleted file has no parent commit")?
            .tree()
            .context("failed to get parent tree")?
    } else {
        commit.tree().context("failed to get commit tree")?
    };
    let entry = tree
        .get_path(std::path::Path::new(file_path))
        .with_context(|| format!("path not in commit: {file_path}"))?;
    read_blob(repo, entry.id())
}

/// The file's contents as of `oid`.
///
/// Which side to read is decided here rather than taken from the caller. A path
/// deleted in a commit is not in that commit's own tree — its content is in the
/// parent's — and the repository already knows which case this is.
/// [`load_commit_file_blob`] is told instead, because the TUI has the status
/// beside the row it is acting on; a request arriving over the wire has no such
/// thing to be trusted with, and asking for it would add an input to validate
/// for an answer that can simply be looked up.
pub fn load_commit_file(repo: &Repository, oid: Oid, file_path: &str) -> Result<String> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let path = std::path::Path::new(file_path);
    let entry = match commit
        .tree()
        .context("failed to get commit tree")?
        .get_path(path)
    {
        Ok(entry) => entry,
        // Not in this commit's tree: the commit is what removed it, so the
        // contents worth showing are the ones it removed.
        //
        // Only for a path that is genuinely absent. Any other failure — a tree
        // object that cannot be read — must not fall through to the parent,
        // which would answer with the previous commit's contents as though they
        // were this one's.
        Err(err) if err.code() == git2::ErrorCode::NotFound => commit
            .parent(0)
            .context("failed to find the parent of a commit that removed this path")?
            .tree()
            .context("failed to get parent tree")?
            .get_path(path)
            .with_context(|| format!("path not in commit: {file_path}"))?,
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read tree for {file_path}"));
        }
    };
    read_blob(repo, entry.id())
}

/// A blob as text, refusing one too large to show *before* it is loaded.
///
/// The size comes from the object database's header rather than from the blob,
/// because reading the blob is what there is to avoid: a repository can hold an
/// object larger than this process should hold in memory, and finding that out
/// from `Blob::content()` means having already paid for it. The working-tree
/// path guards the same way, off the file's metadata.
fn read_blob(repo: &Repository, oid: Oid) -> Result<String> {
    let odb = repo.odb().context("failed to open the object database")?;
    let (size, _) = odb
        .read_header(oid)
        .context("failed to read the blob header")?;
    if size > MAX_FILE_VIEW_BYTES {
        anyhow::bail!("file is too large to display");
    }
    let blob = repo.find_blob(oid).context("failed to read blob")?;
    decode_file_view(blob.content())
}
