use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod attach;
mod daemon;
mod init;
pub(crate) mod plugin_cmd;
mod stop;

pub(crate) use attach::run_attach_detached;
pub(crate) use daemon::run_daemon;
pub(crate) use init::run_init;
pub(crate) use stop::run_stop;

/// nightcrow — session daemon for agentic coding
///
/// Run with no subcommand to start the session: a git diff viewer and
/// multi-terminal panes, served to a terminal and to a browser.
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    pub(crate) exec: Vec<String>,

    /// Override the configured browser port.
    #[arg(long)]
    pub(crate) port: Option<u16>,

    /// Override the configured bind address. `0.0.0.0` exposes the server
    /// to the whole network over plain HTTP.
    #[arg(long)]
    pub(crate) bind: Option<String>,

    /// Run the session in the background and return to the shell.
    ///
    /// It gets its own session, so closing this terminal does not stop it.
    /// A service manager should start nightcrow *without* this — backgrounding
    /// is what it does itself.
    ///
    /// With `attach` it makes no difference: attaching starts a backgrounded
    /// session on its own when none is running.
    #[arg(short, long)]
    pub(crate) detach: bool,

    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Write a starter config file to ~/.nightcrow/config.toml
    Init {
        /// Overwrite the config file if it already exists
        #[arg(long)]
        force: bool,
    },
    /// Attach the TUI to the nightcrow session, starting one if none is running.
    ///
    /// The session — which repositories are open, and in what order — belongs
    /// to the daemon, so this starts on whatever it is serving. Leaving does
    /// not end the session.
    ///
    /// A session started this way runs in the background, so it outlives the
    /// TUI that caused it to exist.
    Attach,
    /// Manage plugin executables in ~/.nightcrow/plugins.
    ///
    /// Installing one only puts the binary in place; it stays inert until
    /// config.toml declares it and a startup pane opts in by name.
    Plugin {
        #[command(subcommand)]
        command: plugin_cmd::PluginCommands,
    },
    /// Ask a running daemon to shut down.
    ///
    /// Sends a graceful shutdown request via the daemon socket. The daemon
    /// runs the same shutdown sequence as SIGINT/SIGTERM.
    Stop {
        /// Path to the daemon socket. Defaults to the standard location.
        #[arg(long)]
        socket: Option<PathBuf>,
    },
}
