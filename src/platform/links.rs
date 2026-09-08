//! Platform launchers for already-parsed terminal links.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Child, Stdio};

use super::link_target::{LinkTarget, parse_target};

/// Parse and launch a target. Every child receives fixed arguments directly;
/// no shell or platform association is used for local files.
pub(crate) fn open(raw: &str, base: &Path) -> Result<()> {
    match parse_target(raw)? {
        LinkTarget::Web(url) => open_web(&url),
        LinkTarget::File { path, line, column } => {
            let path = resolve_file(&path, base)?;
            open_editor(&path, line, column)
        }
    }
}

fn resolve_file(path: &Path, base: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        ensure_file(path)?;
        return Ok(path.to_path_buf());
    }
    let relative = path
        .to_str()
        .context("local link path is not valid UTF-8")?;
    crate::git::path::resolve_in_workdir(base, relative)
}

fn ensure_file(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("cannot access linked file {}", path.display()))?;
    if !metadata.is_file() {
        bail!("linked target is not a regular file: {}", path.display());
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Editor {
    Code,
    #[cfg(not(windows))]
    CliLine,
    #[cfg(windows)]
    Plain,
    #[cfg(target_os = "macos")]
    MacText,
}

fn open_editor(path: &Path, line: Option<u32>, column: Option<u32>) -> Result<()> {
    let location = line.map(|line| match column {
        Some(column) => format!("{}:{line}:{column}", path.display()),
        None => format!("{}:{line}", path.display()),
    });
    let mut last_error = None;
    for (program, editor) in editor_candidates() {
        let mut command = Command::new(program);
        match editor {
            Editor::Code => {
                command.arg("--goto").arg(
                    location
                        .as_deref()
                        .unwrap_or_else(|| path.to_str().unwrap_or_default()),
                );
            }
            #[cfg(not(windows))]
            Editor::CliLine => {
                if let Some(line) = line {
                    command.arg("--line").arg(line.to_string());
                    if let Some(column) = column {
                        command.arg("--column").arg(column.to_string());
                    }
                }
                command.arg(path);
            }
            #[cfg(windows)]
            Editor::Plain => {
                command.arg(path);
            }
            #[cfg(target_os = "macos")]
            Editor::MacText => {
                command.arg("-t").arg(path);
            }
        }
        let result = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match result {
            Ok(child) => {
                reap_child(child);
                return Ok(());
            }
            Err(error) => last_error = Some(error),
        }
    }
    let detail = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "no editor found".into());
    bail!("could not open {}: {detail}", path.display());
}

#[cfg(windows)]
fn editor_candidates() -> Vec<(PathBuf, Editor)> {
    let mut candidates = Vec::new();
    for root in [
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        candidates.push((
            root.join("Programs/Microsoft VS Code/Code.exe"),
            Editor::Code,
        ));
        candidates.push((root.join("Microsoft VS Code/Code.exe"), Editor::Code));
    }
    candidates.push((PathBuf::from("notepad.exe"), Editor::Plain));
    candidates
}

#[cfg(not(windows))]
fn editor_candidates() -> Vec<(PathBuf, Editor)> {
    vec![
        (PathBuf::from("code"), Editor::Code),
        (PathBuf::from("gedit"), Editor::CliLine),
        (PathBuf::from("kate"), Editor::CliLine),
        (PathBuf::from("geany"), Editor::CliLine),
        #[cfg(target_os = "macos")]
        (PathBuf::from("open"), Editor::MacText),
    ]
}

fn reap_child(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(windows)]
fn open_web(url: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let target: Vec<u16> = std::ffi::OsStr::new(url)
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both strings are NUL-terminated UTF-16 buffers valid for this call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        bail!(
            "could not open web link: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_web(url: &str) -> Result<()> {
    let mut command = Command::new("open");
    command.arg(url);
    spawn_detached(&mut command).context("starting macOS URL opener")?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_web(url: &str) -> Result<()> {
    let mut command = Command::new("xdg-open");
    command.arg(url);
    spawn_detached(&mut command).context("starting xdg-open")?;
    Ok(())
}

#[cfg(unix)]
fn spawn_detached(command: &mut Command) -> std::io::Result<()> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    reap_child(child);
    Ok(())
}
