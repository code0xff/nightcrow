use anyhow::{Context, Result};
use std::path::Path;

use super::contract::{
    CHECKSUM_ASSET, MAX_BINARY_BYTES, MAX_CHECKSUM_BYTES, PatchVersion, Release, checksum_for,
    platform_asset,
};
use super::http::Client;
use super::replace::replace_target;
use crate::platform::self_replace;

pub(super) fn run(version: Option<&str>) -> Result<()> {
    let requested = version.map(PatchVersion::requested).transpose()?;
    let asset_name = platform_asset(std::env::consts::OS, std::env::consts::ARCH)?;
    let target =
        std::env::current_exe().context("could not locate the running nightcrow binary")?;
    run_with(
        &Client::github(),
        &target,
        PatchVersion::current()?,
        requested,
        asset_name,
    )
}

fn run_with(
    client: &Client,
    target: &Path,
    current: PatchVersion,
    requested: Option<PatchVersion>,
    asset_name: &str,
) -> Result<()> {
    let release = client.release(requested)?;
    let release_version = release.validate(requested)?;
    if requested.is_none() && release_version <= current {
        println!(
            "nightcrow: already up to date ({current}); latest stable release is {release_version}"
        );
        return Ok(());
    }

    let asset = release.asset(asset_name, MAX_BINARY_BYTES)?;
    let expected = expected_digest(client, &release, asset_name)?;
    let parent = target.parent().with_context(|| {
        format!(
            "installed binary has no parent directory: {}",
            target.display()
        )
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".nightcrow-download-")
        .tempfile_in(parent)
        .with_context(|| {
            format!(
                "could not create a temporary file beside {}",
                target.display()
            )
        })?;
    let actual = client.download_file(asset, temporary.as_file_mut())?;
    if actual != expected {
        anyhow::bail!(
            "SHA-256 verification failed for `{asset_name}`; the installed binary was not changed"
        );
    }
    self_replace::make_executable(temporary.path()).with_context(|| {
        format!(
            "could not make downloaded asset executable at {}",
            temporary.path().display()
        )
    })?;

    replace_target(target, move |target| {
        temporary.persist(target).map_err(|error| {
            anyhow::anyhow!(
                "could not install the verified binary at {}: {}",
                target.display(),
                error.error
            )
        })?;
        Ok(())
    })
}

fn expected_digest(client: &Client, release: &Release, asset_name: &str) -> Result<[u8; 32]> {
    let asset = release.asset(asset_name, MAX_BINARY_BYTES)?;
    if let Some(digest) = asset.api_digest()? {
        return Ok(digest);
    }

    let sums = release.asset(CHECKSUM_ASSET, MAX_CHECKSUM_BYTES)?;
    let expected = sums.api_digest()?.with_context(|| {
        format!("`{CHECKSUM_ASSET}` has no trusted SHA-256 digest in the GitHub API response")
    })?;
    let (contents, actual) = client.download_bytes(sums)?;
    if actual != expected {
        anyhow::bail!("SHA-256 verification failed for `{CHECKSUM_ASSET}`");
    }
    let contents = std::str::from_utf8(&contents)
        .with_context(|| format!("`{CHECKSUM_ASSET}` is not valid UTF-8"))?;
    checksum_for(contents, asset_name)
}

#[cfg(test)]
#[path = "release_test_server.rs"]
mod test_server;

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "release_version_tests.rs"]
mod version_tests;

#[cfg(test)]
#[path = "release_checksum_tests.rs"]
mod checksum_tests;
