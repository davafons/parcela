use clap::{Parser, Subcommand};
use parcela::registry::Role;
use std::path::PathBuf;

/// Run each checkout's services at named HTTPS URLs (experimental).
///
/// Start with `parcela init`, then `parcela up`.
#[derive(Parser)]
#[command(version)]
pub struct Args {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Args, Clone)]
pub struct Global {
    /// Checkout directory to act on.
    #[arg(long, global = true, default_value = ".")]
    pub path: PathBuf,
    /// Private state directory. Defaults to PARCELA_STATE_DIR, then XDG_STATE_HOME/parcela, then ~/.local/state/parcela.
    #[arg(long, global = true)]
    pub state_dir: Option<PathBuf>,
    /// Select a registered environment by ID instead of --path (see parcela ls).
    #[arg(long, global = true, conflicts_with = "path")]
    pub workspace: Option<uuid::Uuid>,
    /// Print JSON. The default when stdout is not a terminal.
    #[arg(long, global = true)]
    pub json: bool,
}

/// A parsed invocation with its command resolved.
pub struct Cli {
    pub path: PathBuf,
    pub state_dir: Option<PathBuf>,
    pub workspace: Option<uuid::Uuid>,
    pub json: bool,
    pub command: Command,
}

impl Cli {
    pub fn new(global: Global, command: Command) -> Self {
        Self {
            path: global.path,
            state_dir: global.state_dir,
            workspace: global.workspace,
            json: global.json,
            command,
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Write a minimal parcela.yaml here, guessing the dev server command.
    Init {
        /// Project name. Defaults to the main checkout's directory name.
        #[arg(long)]
        name: Option<String>,
        /// Command that starts the web service on 127.0.0.1:$PORT.
        #[arg(long)]
        command: Option<String>,
    },
    /// Start this checkout's services. Registers the checkout on first use.
    Up {
        /// Preview name for this checkout. Defaults to the jj workspace or Git worktree name.
        #[arg(long, conflicts_with_all = ["primary", "workspace"])]
        name: Option<String>,
        /// Use the project's base URL, even from a linked checkout.
        #[arg(long, conflicts_with = "workspace")]
        primary: bool,
        /// Return once the services are ready and keep them running in the background.
        #[arg(long)]
        detach: bool,
        /// Port for the default service. Other services get free ports.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        /// Process Compose binary.
        #[arg(long, default_value = "process-compose", hide = true)]
        process_compose: String,
        /// Seconds to wait for every service to answer HTTP.
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400), hide = true)]
        readiness_timeout: u64,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Stop the selected environment. Source and data are kept.
    #[command(visible_alias = "stop")]
    Down {
        /// Stop every running environment of this project.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
        /// Stop every running environment of every project.
        #[arg(long, conflicts_with_all = ["workspace", "all"])]
        all_projects: bool,
    },
    /// Stop all services, then start them again from the current parcela.yaml.
    Restart {
        /// Return once the services are ready and keep them running in the background.
        #[arg(long)]
        detach: bool,
        /// Port for the default service. Other services get free ports.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        /// Process Compose binary.
        #[arg(long, default_value = "process-compose", hide = true)]
        process_compose: String,
        /// Seconds to wait for every service to answer HTTP.
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400), hide = true)]
        readiness_timeout: u64,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Show whether this checkout's services are running and answering.
    Status {
        /// Show every registered environment (same as parcela ls).
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
    },
    /// List every registered environment across projects.
    Ls,
    /// Show each service and worker process.
    Services,
    /// Print recent output of one service. Defaults to the web service.
    Logs {
        /// Service or worker name.
        service: Option<String>,
        /// Number of lines, 1 to 10000.
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=10000))]
        tail: u32,
    },
    /// Print the environment URL. It may not be reachable yet.
    Url {
        /// Print this service's URL instead of the default service's.
        #[arg(long)]
        service: Option<String>,
    },
    /// Forget a stopped environment and release its hostnames. Source and data stay on disk.
    Rm {
        /// Forget every stopped environment whose checkout directory no longer exists.
        #[arg(long, conflicts_with = "workspace")]
        prune: bool,
    },
    /// Check whether this machine and checkout are ready for parcela up. Changes nothing.
    Doctor {
        /// Caddy binary to probe instead of caddy on PATH.
        #[arg(long, hide = true)]
        caddy: Option<String>,
        /// Process Compose binary to probe instead of process-compose on PATH.
        #[arg(long, hide = true)]
        process_compose: Option<String>,
    },
    /// Start, stop or check the HTTPS proxy that serves every environment's URL.
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    /// Trust Parcela's local certificate authority so browsers accept its URLs. Asks for your password.
    Trust {
        /// Print the CA certificate path (for curl --cacert) and change nothing.
        #[arg(long)]
        print: bool,
        /// Caddy binary.
        #[arg(long, default_value = "caddy", hide = true)]
        caddy: String,
    },
    /// Run one literal command as the web service instead of parcela.yaml's services.
    Run {
        /// Preview name for this checkout. Defaults to the jj workspace or Git worktree name.
        #[arg(long, conflicts_with_all = ["primary", "workspace"])]
        name: Option<String>,
        /// Use the project's base URL, even from a linked checkout.
        #[arg(long, conflicts_with = "workspace")]
        primary: bool,
        /// Port for the command to listen on.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        /// Seconds to wait for the command to answer HTTP.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=86400), hide = true)]
        readiness_timeout: u64,
        /// The command and its arguments, after --.
        #[arg(last = true, required = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    /// Run a task from parcela.yaml with this environment's variables.
    Task {
        /// Task name from parcela.yaml.
        name: String,
        /// Extra arguments for the task, after --.
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Take back control of services that kept running after Parcela was interrupted.
    Recover {
        /// Return once control is restored and keep the services in the background.
        #[arg(long)]
        detach: bool,
    },
    /// Print this checkout's environment: URLs, paths and ID.
    #[command(hide = true)]
    Context,
    /// Register, list and discover checkouts. Never edits source or VCS history.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Print the agent skill bundled with this binary.
    #[command(long_flag = "skill")]
    Skill,
}

#[derive(Subcommand)]
pub enum ProxyCommand {
    /// Start the HTTPS proxy in the background. parcela up does this when it isn't running.
    Start {
        #[command(flatten)]
        listen: Listen,
    },
    /// Stop the proxy. Running services keep running but lose their URLs until it starts again.
    Stop,
    /// Stop the proxy and start it again, for example after upgrading Parcela or to change --port.
    Restart {
        #[command(flatten)]
        listen: Listen,
    },
    /// Show whether the proxy is running, where it listens and which Parcela started it.
    Status,
    /// Run the proxy in the foreground, for debugging or a boot service. Never uses sudo or trusts a CA.
    #[command(hide = true)]
    Run {
        #[command(flatten)]
        listen: Listen,
    },
}

#[derive(clap::Args)]
pub struct Listen {
    /// HTTPS port to serve on, saved for later starts. Defaults to the saved port, then PARCELA_HTTPS_PORT, then 1443.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
    pub port: Option<u16>,
    /// Caddy binary.
    #[arg(long, default_value = "caddy", hide = true)]
    pub caddy: String,
}

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// List sibling jj workspaces and Git worktrees without registering them.
    Discover,
    /// Register this checkout explicitly. parcela up does this automatically.
    Adopt {
        /// primary uses the project URL; preview prefixes it with --label.
        #[arg(long, value_enum)]
        role: Role,
        /// Name for this checkout; becomes part of a preview's hostname.
        #[arg(long)]
        label: String,
    },
    /// List this project's registered checkouts.
    List {
        /// List every project's checkouts (same as parcela ls).
        #[arg(long)]
        all: bool,
    },
    /// Print this checkout's environment (same as parcela context).
    Inspect,
}
