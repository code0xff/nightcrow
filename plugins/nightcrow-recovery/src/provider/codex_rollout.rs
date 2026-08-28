//! Pure parsing of codex rollout (`*.jsonl`) records and rollout file names.
//!
//! Split out of `codex.rs` so the record grammar can be exercised without a
//! filesystem, and so every file stays inside the project's 300-line limit.
//!
//! Every rollout line has the shape
//! `{"timestamp":..,"ordinal":N,"type":"<tag>","payload":{..}}`. Only three
//! tags matter to recovery; everything else — including tags added by a
//! future codex release — is ignored silently, because an adapter that fails
//! on unknown records would break on every upgrade.

use crate::provider::reset_epoch_from_json;
use serde_json::Value;

/// Longest line handed to the JSON parser. Real rollout records are far smaller;
/// a longer one is a corrupt or concatenated stream, and parsing it would only
/// spend memory on garbage.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// Longest accepted session id. A uuid is 36 bytes; the slack covers a future id
/// format, and the cap exists because the id becomes a command-line argument.
const MAX_SESSION_ID_BYTES: usize = 128;

/// Longest remembered `rate_limit_reached_type`. It is a provider enum name, so
/// anything longer is not one and is not worth keeping.
const MAX_REACHED_TYPE_BYTES: usize = 64;

/// The one `codex_error_info` value that means "usage limit". Any other value is
/// a different failure mode that waiting cannot fix, so it is not ours.
pub const USAGE_LIMIT_ERROR_INFO: &str = "usage_limit_exceeded";

/// Payload keys that may carry the session id, most specific first.
const SESSION_ID_KEYS: [&str; 3] = ["id", "session_id", "conversation_id"];

/// Where the deadline lives inside a `token_count` payload.
const RESETS_AT_PATH: [&str; 3] = ["rate_limits", "primary", "resets_at"];

const ROLLOUT_PREFIX: &str = "rollout-";
const ROLLOUT_EXT: &str = ".jsonl";

/// A codex thread id is a uuid: five dash-separated hex groups of these lengths.
/// The rollout file name ends with it, and the timestamp before it also contains
/// dashes, so the uuid is recognised by shape rather than by position.
const UUID_GROUP_LENS: [usize; 5] = [8, 4, 4, 4, 12];

/// A rollout record this adapter acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    /// First line of a session. `id` is `None` when no payload key carried a
    /// usable id, in which case the caller falls back to the file name.
    SessionMeta { id: Option<String> },
    /// A usage snapshot. `resets_at` is already validated as a plausible
    /// absolute unix second, or `None`.
    TokenCount {
        resets_at: Option<i64>,
        reached_type: Option<String>,
    },
    /// A turn that ended because the usage limit was hit.
    UsageLimit,
}

/// Classify one rollout line.
///
/// Returns `None` for an over-long line, malformed JSON, a missing or non-string
/// `type`, a tag this adapter does not act on, and a `turn_complete` that is not
/// a usage limit. `now_epoch` is needed only to sanity-check a deadline.
pub fn classify_line(line: &str, now_epoch: i64) -> Option<Record> {
    if line.len() > MAX_RECORD_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let tag = value.get("type")?.as_str()?;
    match tag {
        "session_meta" => Some(Record::SessionMeta {
            id: value.get("payload").and_then(session_id_from_payload),
        }),
        "token_count" => {
            let payload = value.get("payload")?;
            Some(Record::TokenCount {
                resets_at: reset_epoch_from_json(payload, &RESETS_AT_PATH, now_epoch),
                reached_type: reached_type_from_payload(payload),
            })
        }
        "turn_complete" => {
            let info = value
                .get("payload")?
                .get("error")?
                .get("codex_error_info")?
                .as_str()?;
            (info == USAGE_LIMIT_ERROR_INFO).then_some(Record::UsageLimit)
        }
        _ => None,
    }
}

/// The session id a `session_meta` payload carries, if any key holds a valid one.
fn session_id_from_payload(payload: &Value) -> Option<String> {
    SESSION_ID_KEYS
        .iter()
        .filter_map(|key| payload.get(key).and_then(Value::as_str))
        .find(|id| valid_session_id(id))
        .map(str::to_string)
}

/// Which rate-limit window codex says was reached, when it is a short plain
/// string. A long or non-ASCII value is not an enum name and is dropped.
fn reached_type_from_payload(payload: &Value) -> Option<String> {
    let raw = payload
        .get("rate_limits")?
        .get("rate_limit_reached_type")?
        .as_str()?;
    let ok = !raw.is_empty()
        && raw.len() <= MAX_REACHED_TYPE_BYTES
        && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    ok.then(|| raw.to_string())
}

/// Whether an id is safe to hand back as a command-line argument.
///
/// A session id reaches the host as `codex resume <id>`, so it must not be
/// empty, must be bounded, and must contain nothing but characters a shell and
/// an argv both treat as ordinary.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SESSION_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The thread uuid inside `rollout-<timestamp>-<uuid>.jsonl`.
///
/// The timestamp is itself dash-separated, so the uuid is taken as the trailing
/// five groups and only accepted when each has the expected hex shape. Returns
/// `None` for anything that is not a rollout file name.
pub fn session_id_from_filename(name: &str) -> Option<String> {
    let core = name
        .strip_prefix(ROLLOUT_PREFIX)?
        .strip_suffix(ROLLOUT_EXT)?;
    let groups: Vec<&str> = core.split('-').collect();
    if groups.len() < UUID_GROUP_LENS.len() {
        return None;
    }
    let uuid = &groups[groups.len() - UUID_GROUP_LENS.len()..];
    for (group, len) in uuid.iter().zip(UUID_GROUP_LENS) {
        if group.len() != len || !group.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
    }
    Some(uuid.join("-"))
}

#[cfg(test)]
#[path = "codex_rollout_tests.rs"]
mod tests;
