//! `nightcrow update` — install a verified release binary or explicit source.

use anyhow::Result;
use std::path::PathBuf;

mod contract;
mod http;
mod release;
mod replace;
mod source;

pub(crate) fn run_update(
    version: Option<String>,
    path: Option<PathBuf>,
    git: Option<String>,
) -> Result<()> {
    if path.is_some() || git.is_some() {
        source::run(path.as_deref(), git.as_deref())
    } else {
        release::run(version.as_deref())
    }
}
