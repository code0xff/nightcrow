//! The two configured tables a hub is spawned with, and replacing them.
//!
//! Separate from the served set because they answer a different question. The
//! catalog proper is "which repositories exist"; this is "what does a repository
//! start as", which changes for an entirely different reason — the user edited
//! `config.toml` — and reaches only the hubs spawned afterwards.

use super::Catalog;
use std::sync::{Arc, Mutex};

impl Catalog {
    /// Like [`Catalog::new`], but every terminal hub it spawns runs `startup`
    /// as its startup terminals.
    pub fn with_startup(startup_commands: Vec<crate::config::StartupCommand>) -> Self {
        Self {
            startup_commands: Mutex::new(startup_commands),
            ..Self::default()
        }
    }

    /// Like [`Catalog::with_startup`], and every hub is also given the
    /// `[[plugin]]` table its startup commands may name.
    ///
    /// Paired with the startup commands rather than set separately, because the
    /// two are one decision: a plugin is only ever reachable through a startup
    /// command's `plugin =`, so a catalog with one and not the other is a
    /// half-configured session.
    pub fn with_startup_and_plugins(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
    ) -> Self {
        Self::with_startup_plugins_and_exec(startup_commands, plugins, Vec::new())
    }

    /// Like [`Catalog::with_startup_and_plugins`], remembering the `--exec`
    /// commands that were merged into `startup_commands`.
    ///
    /// Kept apart from the merged list rather than derived from it: a reload
    /// re-reads the file and has to arrive at the same combined list, which means
    /// knowing which of the panes came from the command line — nothing in the
    /// merged list says.
    pub fn with_startup_plugins_and_exec(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        cli_startup: Vec<String>,
    ) -> Self {
        Self {
            startup_commands: Mutex::new(startup_commands),
            plugins: Mutex::new(plugins),
            cli_startup,
            ..Self::default()
        }
    }

    /// Replace both configured tables, as a config reload does.
    ///
    /// `file_startup` is the file's `[[startup_command]]` table alone; the
    /// remembered `--exec` panes are merged back on here, so the list a newly
    /// opened repository gets is the one a restart would have produced. A merge
    /// that would exceed the pane cap is refused and *neither* table is
    /// replaced — a reload does not half-apply.
    ///
    /// Only the hubs spawned after this see the startup list. Telling the ones
    /// already running is the caller's job (see
    /// [`crate::web::viewer::reload`]) because it means restarting plugin
    /// children, which is not a catalog concern. The entries to tell are returned
    /// rather than fetched afterwards, which is what makes the split safe — see
    /// below.
    ///
    /// Taken under the mutation lock, the same one every rebuild holds. Without
    /// it a repository opened in the same beat could fall between the two halves:
    /// its hub reads the old tables while the swap is still to come, and the
    /// swap's snapshot is taken while its entry is still to be installed. Nobody
    /// would then tell that hub, and it would run the previous `[[plugin]]` table
    /// for as long as it stayed open. Holding the lock leaves only the two
    /// orderings that are both correct: the open lands first and is in the
    /// snapshot, or it lands second and reads the new tables.
    pub fn set_config_tables(
        &self,
        file_startup: &[crate::config::StartupCommand],
        plugins: Vec<crate::config::PluginConfig>,
    ) -> anyhow::Result<Vec<Arc<super::RepoEntry>>> {
        // Merged before any lock is taken, so a refusal leaves both tables
        // exactly as they were.
        let merged = crate::config::merge_startup_commands(file_startup, &self.cli_startup)?;
        let _mutation = self.mutation.lock().expect("catalog mutation poisoned");
        *self
            .startup_commands
            .lock()
            .expect("catalog startup poisoned") = merged;
        *self.plugins.lock().expect("catalog plugins poisoned") = plugins;
        Ok(self.entries())
    }

    /// The `[[plugin]]` table as it stands, for the caller that has to tell the
    /// running hubs about it.
    pub fn plugins(&self) -> Vec<crate::config::PluginConfig> {
        self.plugins
            .lock()
            .expect("catalog plugins poisoned")
            .clone()
    }

    /// The merged startup list as it stands — configured panes then `--exec`
    /// ones. What the next hub will be given.
    pub fn startup_commands(&self) -> Vec<crate::config::StartupCommand> {
        self.startup_commands
            .lock()
            .expect("catalog startup poisoned")
            .clone()
    }
}
