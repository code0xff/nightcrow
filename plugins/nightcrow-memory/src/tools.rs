//! The tools an agent is offered, and what each does to the store.

use crate::render;
use crate::store::{
    DEFAULT_BOARD_TTL_SECS, DEFAULT_RESULTS, MAX_BOARD_TTL_SECS, MAX_BODY_BYTES, MAX_RESULTS,
    MAX_TAGS, Store,
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

/// Who is calling and when, fixed for one call.
pub struct Caller<'a> {
    pub author: &'a str,
    pub now: i64,
}

/// The tool list as `tools/list` returns it.
pub fn definitions() -> Value {
    let limit = json!({
        "type": "integer", "minimum": 1, "maximum": MAX_RESULTS,
        "description": format!("How many to return (default {DEFAULT_RESULTS}).")
    });
    json!([
        {
            "name": "memory_write",
            "description": "Save a durable fact about this repository for the other agents \
                working in it: a convention, a decision, a pitfall, where something lives. \
                One fact per note. Do not save secrets, credentials or anything personal — \
                every agent in this repository can read it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "maxLength": MAX_BODY_BYTES,
                              "description": "The fact, written to stand on its own." },
                    "tags": { "type": "array", "maxItems": MAX_TAGS, "items": { "type": "string" },
                              "description": "Short topic words to find it by." }
                },
                "required": ["text"]
            }
        },
        {
            "name": "memory_search",
            "description": "Find notes the agents in this repository have saved, by words in \
                their text or tags. Search before starting work in an unfamiliar area.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to look for." },
                    "limit": limit
                },
                "required": ["query"]
            }
        },
        {
            "name": "memory_recent",
            "description": "List the most recently saved notes for this repository.",
            "inputSchema": { "type": "object", "properties": { "limit": limit } }
        },
        {
            "name": "memory_delete",
            "description": "Delete a note that is wrong or no longer true, by the id shown \
                beside it.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"]
            }
        },
        {
            "name": "board_post",
            "description": "Tell the other agents what you are working on right now, so two of \
                you do not edit the same thing. Replaces your previous post and expires on its own.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "maxLength": MAX_BODY_BYTES },
                    "ttl_minutes": {
                        "type": "integer", "minimum": 1, "maximum": MAX_BOARD_TTL_SECS / 60,
                        "description": format!(
                            "Minutes until it expires (default {}).", DEFAULT_BOARD_TTL_SECS / 60)
                    }
                },
                "required": ["text"]
            }
        },
        {
            "name": "board_read",
            "description": "See what the other agents in this repository say they are working \
                on right now.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

/// Run one tool. The text is what the agent reads; an error is refused input or
/// a store failure, and reaches the agent as a tool error rather than ending
/// the session.
pub fn call(store: &mut Store, caller: &Caller<'_>, name: &str, args: &Value) -> Result<String> {
    match name {
        "memory_write" => {
            let id = store.write_note(
                text_arg(args, "text")?,
                &tags_arg(args)?,
                caller.author,
                caller.now,
            )?;
            Ok(format!("Saved as note #{id}."))
        }
        "memory_search" => {
            let found = store.search(text_arg(args, "query")?, limit_arg(args)?)?;
            Ok(render::notes(&found, caller.now, "No notes match."))
        }
        "memory_recent" => {
            let found = store.recent(limit_arg(args)?)?;
            Ok(render::notes(
                &found,
                caller.now,
                "No notes have been saved for this repository.",
            ))
        }
        "memory_delete" => {
            let id = int_arg(args, "id")?.ok_or_else(|| anyhow!("`id` is required"))?;
            if store.delete_note(id)? {
                Ok(format!("Deleted note #{id}."))
            } else {
                bail!("there is no note #{id}")
            }
        }
        "board_post" => {
            let ttl = match int_arg(args, "ttl_minutes")? {
                Some(minutes) => minutes.saturating_mul(60),
                None => DEFAULT_BOARD_TTL_SECS,
            };
            store.post_board(text_arg(args, "text")?, caller.author, caller.now, ttl)?;
            Ok(format!("Posted as {}.", caller.author))
        }
        "board_read" => Ok(render::board(&store.board(caller.now)?, caller.now)),
        other => bail!("unknown tool: {other}"),
    }
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    match args.get(key) {
        Some(Value::String(text)) => Ok(text),
        Some(_) => bail!("`{key}` must be a string"),
        None => bail!("`{key}` is required"),
    }
}

fn int_arg(args: &Value, key: &str) -> Result<Option<i64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| anyhow!("`{key}` must be a whole number")),
    }
}

fn limit_arg(args: &Value) -> Result<usize> {
    match int_arg(args, "limit")? {
        None => Ok(DEFAULT_RESULTS),
        Some(limit) if limit >= 1 => Ok(usize::try_from(limit).unwrap_or(MAX_RESULTS)),
        Some(limit) => bail!("`limit` must be at least 1, not {limit}"),
    }
}

fn tags_arg(args: &Value) -> Result<Vec<String>> {
    match args.get("tags") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("every tag must be a string"))
            })
            .collect(),
        Some(_) => bail!("`tags` must be a list of strings"),
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
