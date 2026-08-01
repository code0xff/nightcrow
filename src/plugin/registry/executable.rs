use anyhow::{Context, Result};
use std::path::Path;

#[cfg(unix)]
const PLUGIN_MODE: u32 = 0o700;

pub(super) fn validate_source(source: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source).with_context(|| {
        format!(
            "plugin source {} cannot be read; it must be an existing executable file",
            source.display()
        )
    })?;
    let metadata = if metadata.file_type().is_symlink() {
        std::fs::metadata(source)
            .with_context(|| format!("plugin source {} is a broken symlink", source.display()))?
    } else {
        metadata
    };
    anyhow::ensure!(
        metadata.is_file(),
        "plugin source {} is not a regular file",
        source.display()
    );
    anyhow::ensure!(
        is_executable(source),
        "plugin source {} is not executable by the current user; chmod +x it first",
        source.display()
    );
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // access(2) accounts for the current user's owner, group, and other bits.
    unsafe { libc::access(path.as_ptr(), libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    let pathext = std::env::var_os("PATHEXT").unwrap_or_else(|| {
        std::ffi::OsString::from(".EXE;.CMD;.BAT;.COM;.PS1;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC")
    });
    let extension = format!(".{}", extension.to_ascii_uppercase());
    pathext.to_str().is_some_and(|entries| {
        entries
            .split(';')
            .any(|entry| entry.eq_ignore_ascii_case(&extension))
    })
}

pub(super) fn restrict_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(path, std::fs::Permissions::from_mode(PLUGIN_MODE))
            .with_context(|| format!("restricting permissions on {}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
