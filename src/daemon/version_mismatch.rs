use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VersionMismatch {
    client: String,
    daemon: String,
}

impl VersionMismatch {
    pub(crate) fn new(client: String, daemon: String) -> Self {
        Self { client, daemon }
    }

    pub(crate) fn from_legacy_error(message: &str, current: &str) -> Option<Self> {
        let body = message.strip_prefix("client is ")?;
        let (client, daemon) = body.split_once(", daemon is ")?;
        if client != current
            || client == daemon
            || !is_supported_version(client)
            || !is_supported_version(daemon)
        {
            return None;
        }
        Some(Self::new(client.to_owned(), daemon.to_owned()))
    }
}

impl fmt::Display for VersionMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "client is {}, daemon is {}",
            printable(&self.client),
            printable(&self.daemon)
        )
    }
}

impl std::error::Error for VersionMismatch {}

fn is_supported_version(value: &str) -> bool {
    let Some(patch) = value.strip_prefix("0.1.") else {
        return false;
    };
    !patch.is_empty()
        && patch.bytes().all(|byte| byte.is_ascii_digit())
        && (patch.len() == 1 || !patch.starts_with('0'))
        && patch.parse::<u64>().is_ok()
}

fn printable(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}

#[cfg(test)]
#[path = "version_mismatch_tests.rs"]
mod tests;
