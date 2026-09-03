use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::time::Duration;

use super::contract::{Asset, PatchVersion, Release};

const GITHUB_API: &str = "https://api.github.com";
const RELEASE_REPOSITORY: &str = "code0xff/nightcrow";
const API_BODY_LIMIT: u64 = 10 * 1024 * 1024;
const USER_AGENT: &str = concat!("nightcrow/", env!("CARGO_PKG_VERSION"));

pub(super) struct Client {
    agent: ureq::Agent,
    api_base: String,
    allow_http: bool,
}

impl Client {
    pub(super) fn github() -> Self {
        Self::new(GITHUB_API, false)
    }

    #[cfg(test)]
    pub(super) fn test(api_base: &str) -> Self {
        Self::new(api_base, true)
    }

    fn new(api_base: &str, allow_http: bool) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(!allow_http)
            .timeout_global(Some(Duration::from_secs(10 * 60)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            api_base: api_base.trim_end_matches('/').to_owned(),
            allow_http,
        }
    }

    pub(super) fn release(&self, requested: Option<PatchVersion>) -> Result<Release> {
        let endpoint = match requested {
            Some(version) => format!("releases/tags/{}", version.tag_name()),
            None => "releases/latest".to_owned(),
        };
        let url = format!("{}/repos/{RELEASE_REPOSITORY}/{endpoint}", self.api_base);
        let mut response = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", USER_AGENT)
            .call()
            .with_context(|| format!("could not fetch release metadata from {url}"))?;
        let bytes = read_bounded(response.body_mut().as_reader(), API_BODY_LIMIT)
            .context("GitHub release metadata exceeded the size limit or could not be read")?;
        serde_json::from_slice(&bytes).context("GitHub returned invalid release metadata")
    }

    pub(super) fn download_file(
        &self,
        asset: &Asset,
        destination: &mut std::fs::File,
    ) -> Result<[u8; 32]> {
        let digest = self.stream_asset(asset, destination)?;
        destination
            .flush()
            .and_then(|()| destination.sync_all())
            .with_context(|| format!("could not flush downloaded asset `{}`", asset.name))?;
        Ok(digest)
    }

    pub(super) fn download_bytes(&self, asset: &Asset) -> Result<(Vec<u8>, [u8; 32])> {
        let mut bytes = Vec::with_capacity(asset.size as usize);
        let digest = self.stream_asset(asset, &mut bytes)?;
        Ok((bytes, digest))
    }

    fn stream_asset(&self, asset: &Asset, output: &mut impl Write) -> Result<[u8; 32]> {
        self.validate_url(&asset.browser_download_url)?;
        let mut response = self
            .agent
            .get(&asset.browser_download_url)
            .header("Accept", "application/octet-stream")
            .header("User-Agent", USER_AGENT)
            .call()
            .with_context(|| format!("could not download release asset `{}`", asset.name))?;
        let mut reader = response.body_mut().as_reader();
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader
                .read(&mut buffer)
                .with_context(|| format!("could not read release asset `{}`", asset.name))?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .context("downloaded asset size overflowed")?;
            if total > asset.size {
                anyhow::bail!(
                    "release asset `{}` exceeded its declared size {}",
                    asset.name,
                    asset.size
                );
            }
            hasher.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .with_context(|| format!("could not write release asset `{}`", asset.name))?;
        }
        if total != asset.size {
            anyhow::bail!(
                "release asset `{}` was {total} bytes, expected {}",
                asset.name,
                asset.size
            );
        }
        Ok(hasher.finalize().into())
    }

    fn validate_url(&self, url: &str) -> Result<()> {
        let allowed =
            url.starts_with("https://") || (self.allow_http && url.starts_with("http://"));
        if !allowed {
            anyhow::bail!("release asset URL must use HTTPS: `{url}`");
        }
        Ok(())
    }
}

fn read_bounded(mut reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .context("could not read HTTP response body")?;
    if bytes.len() as u64 > limit {
        anyhow::bail!("HTTP response body exceeds {limit} bytes");
    }
    Ok(bytes)
}
