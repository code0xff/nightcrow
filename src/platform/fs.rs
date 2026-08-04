//! Filesystem permissions seam. Unix-only APIs like `PermissionsExt` are
//! kept here so call sites stay platform-agnostic. Where Windows has no
//! equivalent, the no-op is documented inline.

use std::path::Path;

/// Restrict a file to owner-only access. On Unix this sets mode 0o600. On
/// Windows there is no portable equivalent of `chmod 600` — the file inherits
/// its ACL from the parent directory and `std::fs` exposes no per-file
/// permission setter — so the call is a documented no-op. Operators who need
/// the guarantee on Windows should place the state directory in an
/// ACL-restricted location.
pub fn set_owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(%err, ?path, "could not set owner-only permissions on session file");
        }
    }
    #[cfg(not(unix))]
    {
        // Windows: no portable per-file permission API. See doc comment above.
        let _ = path;
    }
}

/// Write `data` to `path` atomically: write to a sibling temp file, then
/// rename over the target. The temp file is in the same directory so the
/// rename is atomic on the same filesystem. Permissions are restricted to
/// owner-only before the rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
    })?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "session".into())
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    set_owner_only(&tmp);
    std::fs::rename(&tmp, path)?;
    Ok(())
}
