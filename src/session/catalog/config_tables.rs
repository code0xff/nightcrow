//! The two configured tables a hub is spawned with, and replacing them.
//!
//! Separate from the served set because they answer a different question. The
//! catalog proper is "which repositories exist"; this is "what does a repository
//! start as", which changes when the user edits `config.toml` and reaches only
//! the hubs spawned afterwards.

use super::Catalog;
use std::sync::Arc;

impl Catalog {
    /// Like [`Catalog::new`], with startup terminals and their plugin table.
    #[cfg(test)]
    pub fn with_startup_and_plugins(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
    ) -> Self {
        Self::with_startup_plugins_and_exec(startup_commands, plugins, Vec::new())
    }

    /// Test constructor that also remembers the `--exec` commands merged into
    /// `startup_commands`.
    ///
    /// Kept apart from the merged list rather than derived from it: a reload
    /// re-reads the file and has to arrive at the same combined list.
    #[cfg(test)]
    pub fn with_startup_plugins_and_exec(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        cli_startup: Vec<String>,
    ) -> Self {
        Self {
            runtime: std::sync::Mutex::new(super::CatalogRuntime::configured(
                startup_commands,
                plugins,
                cli_startup,
                crate::config::ShellConfig::default(),
                super::empty_status_payload,
            )),
            ..Self::default()
        }
    }

    /// Construct the catalog tables and shell used by newly spawned panes.
    pub fn with_startup_plugins_exec_and_shell(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        cli_startup: Vec<String>,
        shell: crate::config::ShellConfig,
        status_encoder: crate::session::StatusEncoder,
    ) -> Self {
        Self {
            runtime: std::sync::Mutex::new(super::CatalogRuntime::configured(
                startup_commands,
                plugins,
                cli_startup,
                shell,
                status_encoder,
            )),
            ..Self::default()
        }
    }

    /// Replace both configured tables, as a config reload does.
    ///
    /// `file_startup` is the file's `[[startup_command]]` table alone; the
    /// remembered `--exec` panes are merged back on here. A merge that would
    /// exceed the pane cap is refused and *neither* table is replaced.
    ///
    /// Only the hubs spawned after this see the startup list. Telling the ones
    /// already running is the caller's job (see [`crate::session::reload`]).
    /// The entries to tell are returned rather than fetched afterwards.
    ///
    /// Taken under the facade transaction, the same one every membership
    /// commit holds. Without it a repository opened in the same beat could
    /// fall between the two halves:
    /// its hub reads the old tables while the swap is still to come, and the
    /// swap's snapshot is taken while its entry is still to be installed.
    pub fn set_config_tables(
        &self,
        file_startup: &[crate::config::StartupCommand],
        plugins: Vec<crate::config::PluginConfig>,
    ) -> anyhow::Result<Vec<Arc<super::RepoEntry>>> {
        let _transaction = self
            .transaction
            .lock()
            .expect("catalog transaction poisoned");
        self.runtime
            .lock()
            .expect("catalog runtime poisoned")
            .replace_config(file_startup, plugins)
    }

    /// The `[[plugin]]` table as it stands, for the caller that has to tell the
    /// running hubs about it.
    #[cfg(test)]
    pub fn plugins(&self) -> Vec<crate::config::PluginConfig> {
        self.runtime
            .lock()
            .expect("catalog runtime poisoned")
            .plugins()
    }

    /// The merged startup list as it stands — configured panes then `--exec`
    /// ones. What the next hub will be given.
    #[cfg(test)]
    pub fn startup_commands(&self) -> Vec<crate::config::StartupCommand> {
        self.runtime
            .lock()
            .expect("catalog runtime poisoned")
            .startup_commands()
    }
}
