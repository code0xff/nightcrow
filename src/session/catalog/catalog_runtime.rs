//! Live workers corresponding to the catalog's pure membership.

use super::catalog_ids::{Member, RepoEntry};
use super::{display_path, empty_status_payload, repo_name};
use crate::session::StatusEncoder;
use crate::session::runtime::RepoRuntime;
use crate::session::terminal::TerminalHub;
use std::sync::Arc;

pub(super) struct CatalogRuntime {
    entries: Vec<Arc<RepoEntry>>,
    startup_commands: Vec<crate::config::StartupCommand>,
    cli_startup: Vec<String>,
    plugins: Vec<crate::config::PluginConfig>,
    shell: crate::config::ShellConfig,
    ownership: Arc<crate::session::size_owner::SizeOwnership>,
    status_encoder: StatusEncoder,
}

impl Default for CatalogRuntime {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            startup_commands: Vec::new(),
            cli_startup: Vec::new(),
            plugins: Vec::new(),
            shell: crate::config::ShellConfig::default(),
            ownership: Arc::new(crate::session::size_owner::SizeOwnership::default()),
            status_encoder: empty_status_payload,
        }
    }
}

impl CatalogRuntime {
    pub(super) fn configured(
        startup_commands: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        cli_startup: Vec<String>,
        shell: crate::config::ShellConfig,
        status_encoder: StatusEncoder,
    ) -> Self {
        Self {
            startup_commands,
            plugins,
            cli_startup,
            shell,
            status_encoder,
            ..Self::default()
        }
    }

    pub(super) fn reconcile(&mut self, members: Vec<Member>) -> Vec<Arc<RepoEntry>> {
        let previous = std::mem::take(&mut self.entries);
        let mut next = Vec::with_capacity(members.len());
        for member in members {
            if let Some(existing) = previous.iter().find(|entry| entry.path == member.path) {
                next.push(Arc::clone(existing));
                continue;
            }
            next.push(Arc::new(RepoEntry {
                name: repo_name(&member.path),
                display_path: display_path(&member.path),
                runtime: RepoRuntime::spawn(&member.path, self.status_encoder),
                terminals: TerminalHub::spawn(
                    &member.path,
                    self.startup_commands.clone(),
                    self.plugins.clone(),
                    self.shell.clone(),
                    Arc::clone(&self.ownership),
                ),
                id: member.id,
                path: member.path,
            }));
        }
        let retired = previous
            .into_iter()
            .filter(|old| !next.iter().any(|new| Arc::ptr_eq(new, old)))
            .collect();
        self.entries = next;
        retired
    }

    pub(super) fn replace_config(
        &mut self,
        file_startup: &[crate::config::StartupCommand],
        plugins: Vec<crate::config::PluginConfig>,
    ) -> anyhow::Result<Vec<Arc<RepoEntry>>> {
        let merged = crate::config::merge_startup_commands(file_startup, &self.cli_startup)?;
        self.startup_commands = merged;
        self.plugins = plugins;
        Ok(self.entries.clone())
    }

    pub(super) fn entries(&self) -> &[Arc<RepoEntry>] {
        &self.entries
    }

    pub(super) fn take_entries(&mut self) -> Vec<Arc<RepoEntry>> {
        std::mem::take(&mut self.entries)
    }

    #[cfg(test)]
    pub(super) fn startup_commands(&self) -> Vec<crate::config::StartupCommand> {
        self.startup_commands.clone()
    }

    #[cfg(test)]
    pub(super) fn plugins(&self) -> Vec<crate::config::PluginConfig> {
        self.plugins.clone()
    }
}
