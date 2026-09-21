use clap::{CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

mod attach;
mod daemon;
mod init;
pub(crate) mod plugin_cmd;
pub(crate) mod status;
mod stop;
mod update;

pub(crate) use attach::run_attach_detached;
pub(crate) use daemon::run_daemon;
pub(crate) use init::run_init;
pub(crate) use status::run_status;
pub(crate) use stop::run_stop;
pub(crate) use update::run_update;

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

    /// Run the session as a background daemon and return to the shell.
    ///
    /// It gets its own session, so closing this terminal does not stop it.
    /// A service manager should start nightcrow *without* this — backgrounding
    /// is what it does itself.
    ///
    /// With `attach` it makes no difference: attaching starts a background
    /// daemon on its own when none is running.
    #[arg(short = 'd', long = "daemon")]
    pub(crate) daemon: bool,

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
    /// Show the state of a running daemon without attaching a client.
    Status {
        /// Path to the daemon socket. Defaults to the standard location.
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Update nightcrow from an official binary release or an explicit source.
    ///
    /// With no source options, downloads the latest stable v0.1.x release.
    /// `--path` and `--git` are development paths that run Cargo instead.
    Update {
        /// Install this official v0.1.x release, including older patch versions.
        #[arg(long, value_name = "0.1.N", conflicts_with_all = ["path", "git"])]
        version: Option<String>,

        /// Install from a local checkout instead of the upstream repository.
        #[arg(long, value_name = "DIR", conflicts_with = "git")]
        path: Option<PathBuf>,

        /// Install from this git repository instead of the upstream one.
        #[arg(long, value_name = "URL", conflicts_with = "path")]
        git: Option<String>,
    },
}

impl Cli {
    /// The session-shaping options this invocation set, named as the user
    /// typed them. Empty when none is in play.
    fn session_options(&self) -> Vec<&'static str> {
        let mut named = Vec::new();
        if !self.exec.is_empty() {
            named.push("--exec");
        }
        if self.port.is_some() {
            named.push("--port");
        }
        if self.bind.is_some() {
            named.push("--bind");
        }
        if self.daemon {
            named.push("--daemon");
        }
        named
    }
}

/// The subcommand an option is a mistake on, if it is one.
///
/// These options only shape a session that is about to start, so pairing them
/// with a subcommand that never starts one is a silent no-op: without this,
/// `nightcrow --port 9000 status` reads as if it addressed that port. `attach`
/// is deliberately absent — it starts a daemon when none is running and passes
/// the options on to it.
fn command_that_starts_no_session(command: &Commands) -> Option<&'static str> {
    match command {
        Commands::Init { .. } => Some("init"),
        Commands::Plugin { .. } => Some("plugin"),
        Commands::Stop { .. } => Some("stop"),
        Commands::Status { .. } => Some("status"),
        Commands::Update { .. } => Some("update"),
        Commands::Attach => None,
    }
}

/// Reject session options on a subcommand that starts no session, in clap's
/// own error shape so the message and exit code match every other misuse.
pub(crate) fn reject_session_options(cli: &Cli) -> Result<(), clap::Error> {
    let Some(command) = cli.command.as_ref() else {
        return Ok(());
    };
    let Some(name) = command_that_starts_no_session(command) else {
        return Ok(());
    };
    let options = cli.session_options();
    if options.is_empty() {
        return Ok(());
    }
    Err(Cli::command().error(
        clap::error::ErrorKind::ArgumentConflict,
        format!(
            "{} cannot be used with `{name}`: {}",
            options.join(", "),
            if options.len() == 1 {
                "it only shapes a session that is starting"
            } else {
                "they only shape a session that is starting"
            },
        ),
    ))
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
