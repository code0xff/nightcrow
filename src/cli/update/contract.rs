use anyhow::{Context, Result};
use serde::Deserialize;
use std::fmt;

pub(super) const CHECKSUM_ASSET: &str = "SHA256SUMS";
pub(super) const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_CHECKSUM_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct PatchVersion(u64);

impl PatchVersion {
    pub(super) fn requested(value: &str) -> Result<Self> {
        parse_version(value, "", "version")
    }

    pub(super) fn tag(value: &str) -> Result<Self> {
        parse_version(value, "v", "release tag")
    }

    pub(super) fn current() -> Result<Self> {
        Self::requested(env!("CARGO_PKG_VERSION"))
            .context("package version is not in the 0.1.x series")
    }

    pub(super) fn tag_name(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for PatchVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "0.1.{}", self.0)
    }
}

fn parse_version(value: &str, prefix: &str, label: &str) -> Result<PatchVersion> {
    let Some(version) = value.strip_prefix(prefix) else {
        anyhow::bail!("invalid {label} `{value}`; expected {prefix}0.1.N");
    };
    let Some(patch) = version.strip_prefix("0.1.") else {
        anyhow::bail!("invalid {label} `{value}`; only the 0.1.x series is allowed");
    };
    if patch.is_empty()
        || !patch.bytes().all(|byte| byte.is_ascii_digit())
        || (patch.len() > 1 && patch.starts_with('0'))
    {
        anyhow::bail!("invalid {label} `{value}`; expected {prefix}0.1.N");
    }
    let patch = patch
        .parse::<u64>()
        .with_context(|| format!("invalid {label} `{value}`; patch number is too large"))?;
    Ok(PatchVersion(patch))
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Release {
    pub(super) tag_name: String,
    pub(super) draft: bool,
    pub(super) prerelease: bool,
    pub(super) assets: Vec<Asset>,
}

impl Release {
    pub(super) fn validate(&self, requested: Option<PatchVersion>) -> Result<PatchVersion> {
        if self.draft || self.prerelease {
            anyhow::bail!(
                "release {} is not a published stable release",
                self.tag_name
            );
        }
        let version = PatchVersion::tag(&self.tag_name)?;
        if let Some(requested) = requested
            && requested != version
        {
            anyhow::bail!(
                "GitHub returned release {} for requested version {}",
                self.tag_name,
                requested
            );
        }
        Ok(version)
    }

    pub(super) fn asset(&self, name: &str, max_size: u64) -> Result<&Asset> {
        let mut matches = self.assets.iter().filter(|asset| asset.name == name);
        let asset = matches
            .next()
            .with_context(|| format!("release {} has no `{name}` asset", self.tag_name))?;
        if matches.next().is_some() {
            anyhow::bail!("release {} has duplicate `{name}` assets", self.tag_name);
        }
        asset.validate(max_size)?;
        Ok(asset)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Asset {
    pub(super) name: String,
    pub(super) browser_download_url: String,
    pub(super) size: u64,
    pub(super) digest: Option<String>,
    pub(super) state: String,
}

impl Asset {
    fn validate(&self, max_size: u64) -> Result<()> {
        if self.state != "uploaded" {
            anyhow::bail!("release asset `{}` is not fully uploaded", self.name);
        }
        if self.size == 0 || self.size > max_size {
            anyhow::bail!(
                "release asset `{}` has invalid size {} (maximum {max_size})",
                self.name,
                self.size
            );
        }
        Ok(())
    }

    pub(super) fn api_digest(&self) -> Result<Option<[u8; 32]>> {
        self.digest
            .as_deref()
            .map(|digest| {
                let value = digest.strip_prefix("sha256:").with_context(|| {
                    format!("release asset `{}` has a non-SHA-256 digest", self.name)
                })?;
                parse_digest(value)
                    .with_context(|| format!("release asset `{}` has an invalid digest", self.name))
            })
            .transpose()
    }
}

pub(super) fn platform_asset(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("nightcrow-x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("nightcrow-x86_64-pc-windows-msvc.exe"),
        ("macos", "x86_64") => Ok("nightcrow-x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("nightcrow-aarch64-apple-darwin"),
        _ => anyhow::bail!("official release binaries do not support {os}/{arch}"),
    }
}

pub(super) fn checksum_for(contents: &str, asset_name: &str) -> Result<[u8; 32]> {
    let mut found = None;
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        let Some(digest) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else { continue };
        if name.trim_start_matches('*') != asset_name {
            continue;
        }
        if fields.next().is_some() || found.is_some() {
            anyhow::bail!("`{CHECKSUM_ASSET}` has an ambiguous entry for `{asset_name}`");
        }
        found = Some(parse_digest(digest).with_context(|| {
            format!("`{CHECKSUM_ASSET}` has an invalid digest for `{asset_name}`")
        })?);
    }
    found.with_context(|| format!("`{CHECKSUM_ASSET}` has no entry for `{asset_name}`"))
}

fn parse_digest(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("expected 64 hexadecimal characters");
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;
