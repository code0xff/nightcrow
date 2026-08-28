use super::super::http_util::json_error;
use super::lookup::redact;

#[derive(serde::Deserialize)]
struct MkdirRequest {
    /// The directory to create the new folder inside.
    path: String,
    /// Must be a single plain path segment.
    name: String,
}

/// Create a new folder inside a directory the picker is browsing.
///
/// The parent is confined only as much as `browse` is, but `name` is held to
/// a single plain segment: separators, `..`, a leading `.` (which also rules
/// out `.git`), and NUL are all rejected. Combined with canonicalizing the
/// parent first, the created folder can only ever land directly under the
/// browsed directory.
pub(in crate::web::viewer::server) fn handle_mkdir(body: &str) -> Vec<u8> {
    let request: MkdirRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return json_error(
                "400 Bad Request",
                "expected a JSON body with a path and a name",
            );
        }
    };
    let name = request.name.trim();
    if name.is_empty() {
        return json_error("400 Bad Request", "a folder name is required");
    }
    if name.starts_with('.') || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return json_error("400 Bad Request", "invalid folder name");
    }
    let parent = crate::platform::paths::expand_tilde(request.path.trim());
    if !parent.is_dir() {
        return json_error("400 Bad Request", "no such directory");
    }
    let base = match parent.canonicalize() {
        Ok(base) => base,
        Err(err) => return redact("mkdir canonicalize", &anyhow::Error::new(err)),
    };
    let target = base.join(name);
    match std::fs::create_dir(&target) {
        Ok(()) => super::encode_response(
            serde_json::json!({
                "path": crate::platform::paths::for_display(&target).into_owned()
            }),
            "could not encode the folder",
        ),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            json_error("409 Conflict", "a folder with that name already exists")
        }
        Err(err) => redact("mkdir", &anyhow::Error::new(err)),
    }
}
