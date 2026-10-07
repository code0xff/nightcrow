mod diff;
mod fixture;
mod fixture_log;
mod identity;
mod status;

use serde::Serialize;

fn json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}
