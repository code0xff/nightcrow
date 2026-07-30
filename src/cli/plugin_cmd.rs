//! `nightcrow plugin` — argument parsing and reporting only. Every filesystem
//! decision lives in [`crate::plugin::registry`].

use anyhow::Result;
use clap::Subcommand;
use std::path::{Path, PathBuf};

use crate::plugin::registry::{self, InstallOutcome, RemoveOutcome};

#[derive(Subcommand)]
pub(crate) enum PluginCommands {
    /// Copy an executable into ~/.nightcrow/plugins
    Install {
        /// Path to the plugin executable
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Install under this name instead of the source file stem
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Replace an already-installed plugin of the same name
        #[arg(long)]
        force: bool,
    },
    /// List installed plugins and how config.toml refers to them
    List,
    /// Delete an installed plugin
    Remove {
        #[arg(value_name = "NAME")]
        name: String,
    },
}

pub(crate) fn run_plugin(command: PluginCommands) -> Result<()> {
    let dir = registry::default_plugins_dir()?;
    match command {
        PluginCommands::Install { path, name, force } => {
            run_install(&dir, &path, name.as_deref(), force)
        }
        PluginCommands::List => run_list(&dir),
        PluginCommands::Remove { name } => run_remove(&dir, &name),
    }
}

fn run_install(dir: &Path, path: &Path, name: Option<&str>, force: bool) -> Result<()> {
    let installed = match registry::install(dir, path, name, force)? {
        InstallOutcome::Created(installed) => {
            println!("Installed {}", installed.display());
            installed
        }
        InstallOutcome::Replaced(installed) => {
            println!("Replaced {}", installed.display());
            installed
        }
        InstallOutcome::AlreadyExists(installed) => {
            println!(
                "A plugin is already installed at {} — left untouched (pass --force to replace).",
                installed.display()
            );
            return Ok(());
        }
    };
    let name = installed
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    println!();
    println!("Add this to ~/.nightcrow/config.toml to declare it:");
    println!();
    print!("{}", registry::config_snippet(&name, &installed));
    println!();
    // Deliberately not written for the user: a plugin that is enabled and
    // opted into can drive that pane's terminal, so both switches stay an
    // explicit, reviewed edit.
    println!(
        "Nothing was written to your config — flip `enabled` yourself, and add \
         `plugin = \"{name}\"` to a [[startup_command]] to opt that pane in. \
         A plugin with no opted-in pane never sees anything."
    );
    Ok(())
}

fn run_list(dir: &Path) -> Result<()> {
    let names = registry::list(dir)?;
    if names.is_empty() {
        println!("No plugins installed in {}", dir.display());
        return Ok(());
    }
    // A broken config must not hide what is on disk — listing files is the
    // thing this command is for, and the config line is extra detail.
    let cfg = match crate::config::load_config() {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            println!("warning: config.toml could not be read ({e:#})");
            println!("Listing installed files only.");
            None
        }
    };
    println!("Installed plugins in {}:", dir.display());
    for name in names {
        match cfg.as_ref() {
            Some(cfg) => println!("  {name}  —  {}", describe(&registry::status(cfg, &name))),
            None => println!("  {name}"),
        }
    }
    Ok(())
}

fn describe(status: &registry::PluginStatus) -> String {
    if !status.declared {
        return "not declared in config.toml".to_string();
    }
    format!(
        "declared, {}, {} startup pane{} opted in",
        if status.enabled {
            "enabled"
        } else {
            "disabled"
        },
        status.opt_ins,
        if status.opt_ins == 1 { "" } else { "s" }
    )
}

fn run_remove(dir: &Path, name: &str) -> Result<()> {
    let references = crate::config::load_config()
        .ok()
        .map(|cfg| registry::status(&cfg, name));
    match registry::remove(dir, name)? {
        RemoveOutcome::Removed(path) => println!("Removed {}", path.display()),
        RemoveOutcome::NotInstalled(name) => {
            println!(
                "No plugin named \"{name}\" is installed in {}",
                dir.display()
            );
        }
    }
    // A `plugin =` opt-in naming a plugin that is no longer declared is a hard
    // config error, so a dangling reference stops the next startup outright.
    if let Some(status) = references.filter(|s| s.declared || s.opt_ins > 0) {
        println!();
        println!("WARNING: config.toml still refers to \"{name}\".");
        if status.declared {
            println!("  - remove its [[plugin]] table (name = \"{name}\")");
        }
        if status.opt_ins > 0 {
            println!(
                "  - remove `plugin = \"{name}\"` from {} [[startup_command]] entr{}",
                status.opt_ins,
                if status.opt_ins == 1 { "y" } else { "ies" }
            );
        }
        println!(
            "Left as they are, the next `nightcrow` start tries to launch a plugin that is \
             no longer installed — and a `plugin =` opt-in whose [[plugin]] table is gone \
             fails config validation outright."
        );
    }
    Ok(())
}
