//! The JSON surgery behind `install`/`uninstall`, as pure functions over
//! [`Value`] so the tricky cases are testable without touching a filesystem.
//!
//! Our entries are identified by [`MARKER`] appearing as a substring of a
//! `command` string. `command` is the only field Claude Code's hook and
//! statusline schema lets us write free text into; a custom key of our own (say
//! `"nightcrowOwned": true`) may be rejected or warned about as unknown by the
//! provider, so it is not a safe place to keep our bookkeeping.

use anyhow::Result;
use serde_json::{Map, Value, json};

/// Substring that marks a settings entry as ours. It is the crate name, which is
/// also the installed binary's name, so `"<exe> hook"` carries it for free.
pub(crate) const MARKER: &str = "nightcrow-recovery";

pub(crate) const HOOKS_KEY: &str = "hooks";
pub(crate) const HOOK_EVENT: &str = "StopFailure";
/// Fires as every turn ends, whatever the outcome. Where `StopFailure` is
/// narrowed to one `error_type`, this one cannot be: "the turn is over" has no
/// sub-kinds to ask for, and the marker it raises means only that.
pub(crate) const TURN_END_EVENT: &str = "Stop";
/// `Stop` carries no error to match on, so the group is the unfiltered one.
pub(crate) const TURN_END_MATCHER: &str = "";
pub(crate) const STATUSLINE_KEY: &str = "statusLine";
const MATCHER_KEY: &str = "matcher";
const COMMAND_KEY: &str = "command";

/// The one `error_type` we ask to be woken for. Least privilege: a rate limit is
/// the only failure this plugin acts on, so payloads for unrelated failures
/// (`authentication_failed`, `billing_error`, ...) never reach this process at
/// all. The consequence is deliberate: transient `overloaded`/`server_error`
/// conditions are recognised from the pane's terminal output instead of here.
pub(crate) const HOOK_MATCHER: &str = "rate_limit";

/// Seconds Claude Code should wait for our hook. The hook only parses one JSON
/// payload and hands it to the running host, so this is already generous; the cap
/// matters because a wedged helper must not sit in the provider's path, and
/// `StopFailure` ignores our output and exit code anyway.
const HOOK_TIMEOUT_SECS: u64 = 5;

/// Columns of gutter around the rendered statusline, so it does not butt against
/// the terminal edge. Matches the value in Claude Code's own documented example.
const STATUSLINE_PADDING: u64 = 2;

/// Quote a path for the POSIX shell these commands are run in.
///
/// Claude Code runs a hook and a `statusLine` through a shell, on every platform
/// — its own documented examples are shell one-liners, and the entries other
/// tools install here are `if [ -f '...' ]; then ...`. So a Windows path cannot
/// be written bare: the shell reads each backslash as an escape, so
/// `C:\Users\me\plugin` arrives as `C:Usersmeplugin` and is simply not found.
/// Single quotes suspend every interpretation the shell would otherwise make,
/// which covers spaces in the path as well.
///
/// A single quote cannot appear inside single quotes, so an embedded one is
/// closed, escaped on its own, and reopened.
fn shell_quoted(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

pub(crate) fn hook_command(exe: &str) -> String {
    format!("{} hook", shell_quoted(exe))
}

pub(crate) fn turn_end_command(exe: &str) -> String {
    format!("{} turn-end", shell_quoted(exe))
}

pub(crate) fn statusline_command(exe: &str) -> String {
    format!("{} statusline", shell_quoted(exe))
}

pub(crate) fn is_ours(command: &str) -> bool {
    command.contains(MARKER)
}

fn command_of(entry: &Value) -> Option<&str> {
    entry.get(COMMAND_KEY).and_then(Value::as_str)
}

fn hook_entry(command: &str) -> Value {
    json!({ "type": "command", "command": command, "timeout": HOOK_TIMEOUT_SECS })
}

fn statusline_entry(command: &str) -> Value {
    json!({ "type": "command", "command": command, "padding": STATUSLINE_PADDING })
}

fn matcher_group(matcher: &str) -> Value {
    let mut group = Map::new();
    group.insert(MATCHER_KEY.to_string(), Value::String(matcher.to_string()));
    group.insert(HOOKS_KEY.to_string(), Value::Array(Vec::new()));
    Value::Object(group)
}

pub(crate) fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn object_mut<'a>(value: &'a mut Value, what: &str) -> Result<&'a mut Map<String, Value>> {
    let found = kind_of(value);
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{what} is a JSON {found}, not an object"))
}

fn array_mut<'a>(value: &'a mut Value, what: &str) -> Result<&'a mut Vec<Value>> {
    let found = kind_of(value);
    value
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("{what} is a JSON {found}, not an array"))
}

/// Add our hook entry and statusline to `settings`, leaving every other key and
/// every array entry we did not add exactly as it was. Returns one line per
/// change plus the `statusLine` value we displaced, which the caller must record
/// before writing so uninstall can put it back.
pub(crate) fn merge_into(settings: &mut Value, exe: &str) -> Result<(Vec<String>, Option<Value>)> {
    let mut changes = Vec::new();
    let root = object_mut(settings, "the settings root")?;

    merge_hook(
        root,
        HOOK_EVENT,
        HOOK_MATCHER,
        &hook_command(exe),
        &mut changes,
    )?;
    merge_hook(
        root,
        TURN_END_EVENT,
        TURN_END_MATCHER,
        &turn_end_command(exe),
        &mut changes,
    )?;

    let mut displaced = None;
    let ours = root
        .get(STATUSLINE_KEY)
        .and_then(command_of)
        .is_some_and(is_ours);
    if !ours {
        let previous = root.get(STATUSLINE_KEY).cloned().unwrap_or(Value::Null);
        let command = statusline_command(exe);
        root.insert(STATUSLINE_KEY.to_string(), statusline_entry(&command));
        changes.push(if previous.is_null() {
            format!("set {STATUSLINE_KEY}: {command}")
        } else {
            format!("replaced {STATUSLINE_KEY} with: {command} (previous value recorded)")
        });
        displaced = Some(previous);
    }

    Ok((changes, displaced))
}

/// Remove exactly what [`merge_into`] added, putting `restore` back as
/// Put one command into one hook event's matcher group, creating whatever is
/// missing and touching nothing else.
fn merge_hook(
    root: &mut Map<String, Value>,
    event: &str,
    matcher: &str,
    command: &str,
    changes: &mut Vec<String>,
) -> Result<()> {
    let hooks = root.entry(HOOKS_KEY).or_insert_with(|| json!({}));
    let hooks = object_mut(hooks, "`hooks`")?;
    let groups = hooks.entry(event).or_insert_with(|| json!([]));
    let groups = array_mut(groups, &format!("`hooks.{event}`"))?;
    let index = match groups
        .iter()
        .position(|g| g.get(MATCHER_KEY).and_then(Value::as_str) == Some(matcher))
    {
        Some(index) => index,
        None => {
            groups.push(matcher_group(matcher));
            groups.len() - 1
        }
    };
    let group = object_mut(&mut groups[index], "the matcher group")?;
    let entries = group.entry(HOOKS_KEY).or_insert_with(|| json!([]));
    let entries = array_mut(entries, "the matcher group's `hooks`")?;
    // An exact command match already present is what makes a second install a
    // no-op instead of a duplicate entry.
    if entries.iter().any(|e| command_of(e) == Some(command)) {
        return Ok(());
    }
    entries.push(hook_entry(command));
    changes.push(format!(
        "added {event} hook for matcher `{matcher}`: {command}"
    ));
    Ok(())
}

/// `statusLine` when it holds a value we recorded. Containers we empty are
/// collapsed so the file returns to its original shape.
pub(crate) fn strip_from(settings: &mut Value, restore: Option<Value>) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    let root = object_mut(settings, "the settings root")?;

    let removed = strip_hooks(root);
    if removed > 0 {
        changes.push(format!("removed {removed} hook command(s)"));
    }

    let ours = root
        .get(STATUSLINE_KEY)
        .and_then(command_of)
        .is_some_and(is_ours);
    if ours {
        match restore {
            Some(previous) if !previous.is_null() => {
                root.insert(STATUSLINE_KEY.to_string(), previous);
                changes.push(format!("restored the previous {STATUSLINE_KEY}"));
            }
            _ => {
                root.remove(STATUSLINE_KEY);
                changes.push(format!("removed our {STATUSLINE_KEY}"));
            }
        }
    }

    Ok(changes)
}

/// Drop our hook commands and return how many went. A `hooks` subtree of an
/// unexpected shape is left alone rather than rewritten: it cannot be ours.
fn strip_hooks(root: &mut Map<String, Value>) -> usize {
    let mut removed = 0usize;
    let mut hooks_empty = false;
    if let Some(hooks) = root.get_mut(HOOKS_KEY).and_then(Value::as_object_mut) {
        for event in [HOOK_EVENT, TURN_END_EVENT] {
            removed += strip_hook_event(hooks, event);
        }
        hooks_empty = hooks.is_empty();
    }
    if hooks_empty && removed > 0 {
        root.remove(HOOKS_KEY);
    }
    removed
}

/// Drop our commands from one hook event, collapsing only what we emptied.
fn strip_hook_event(hooks: &mut Map<String, Value>, event: &str) -> usize {
    let mut removed = 0usize;
    let mut groups_empty = false;
    if let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) {
        let mut drop_indexes = Vec::new();
        for (index, group) in groups.iter_mut().enumerate() {
            let Some(entries) = group.get_mut(HOOKS_KEY).and_then(Value::as_array_mut) else {
                continue;
            };
            let before = entries.len();
            entries.retain(|e| !command_of(e).is_some_and(is_ours));
            if entries.len() == before {
                continue;
            }
            removed += before - entries.len();
            if entries.is_empty() {
                drop_indexes.push(index);
            }
        }
        for index in drop_indexes.into_iter().rev() {
            groups.remove(index);
        }
        groups_empty = groups.is_empty();
    }
    // Only collapse containers we emptied ourselves, so a user's own empty
    // event list survives an uninstall that found nothing of ours.
    if groups_empty && removed > 0 {
        hooks.remove(event);
    }
    removed
}

#[cfg(test)]
#[path = "hooks_merge_tests.rs"]
mod tests;
