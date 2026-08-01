use super::catalog::Catalog;
use super::prefs::PrefsStore;
use crate::git::diff::RepoSnapshot;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

/// Turns one repository snapshot into the payload cached for status clients.
///
/// Encoding belongs to the consuming surface, not the session. Passing the
/// encoder in keeps the status worker transport-neutral while preserving the
/// existing encode-once fan-out behavior.
pub type StatusEncoder = fn(&RepoSnapshot, &HashMap<String, SystemTime>) -> Option<String>;

#[cfg(test)]
pub fn test_status_encoder(_: &RepoSnapshot, _: &HashMap<String, SystemTime>) -> Option<String> {
    Some("{}".to_string())
}

/// Everything required to construct the state one daemon owns.
pub struct SessionOptions {
    pub repos: Vec<String>,
    pub persist: bool,
    pub startup_commands: Vec<crate::config::StartupCommand>,
    pub cli_startup: Vec<String>,
    pub shell: crate::config::ShellConfig,
    pub prefs: PrefsStore,
    pub status_encoder: StatusEncoder,
}

/// Repository, terminal, and shared-preference state independent of transport.
pub struct SessionState {
    pub(super) catalog: Catalog,
    pub(super) persist: bool,
    pub(super) prefs: PrefsStore,
    pub(super) reload_lock: Mutex<()>,
}

impl SessionState {
    #[cfg(test)]
    pub fn new(options: SessionOptions) -> Self {
        Self::with_plugins(options, Vec::new())
    }

    pub fn with_plugins(
        options: SessionOptions,
        plugins: Vec<crate::config::PluginConfig>,
    ) -> Self {
        let catalog = Catalog::with_startup_plugins_exec_and_shell(
            options.startup_commands,
            plugins,
            options.cli_startup,
            options.shell,
            options.status_encoder,
        );
        catalog.set_paths(&options.repos);
        Self {
            catalog,
            persist: options.persist,
            prefs: options.prefs,
            reload_lock: Mutex::new(()),
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn prefs(&self) -> &PrefsStore {
        &self.prefs
    }

    pub fn shutdown(&self) {
        self.catalog.shutdown();
    }
}

impl Drop for SessionState {
    fn drop(&mut self) {
        self.catalog.shutdown();
    }
}
