use super::{ViewerServer, body_of, get, login, post, server_with};

mod arrangement;
mod mkdir;
mod preferences;

fn prefs_server() -> (tempfile::TempDir, ViewerServer, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig::default(),
        Some(dir.path()),
    );
    let token = login(server.addr());
    (dir, server, token)
}
