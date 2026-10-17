use clap::{Parser, Subcommand};
use parcela::registry::Role;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Workspace organizer with owned local runtimes (experimental)"
)]
pub struct Cli {
    /// Checkout directory; defaults to the current directory.
    #[arg(long, global = true, default_value = ".")]
    pub path: PathBuf,
    /// Private registry directory; defaults to PARCELA_STATE_DIR, XDG_STATE_HOME/parcela or ~/.local/state/parcela.
    #[arg(long, global = true)]
    pub state_dir: Option<PathBuf>,
    /// Select an adopted environment by UUID instead of a checkout path.
    #[arg(long, global = true, conflicts_with = "path")]
    pub workspace: Option<uuid::Uuid>,
    /// Print JSON results. Default when stdout is not a terminal.
    #[arg(long, global = true)]
    pub json: bool,
    /// Accepted for compatibility; human output is the default on a terminal.
    #[arg(long, global = true, hide = true)]
    pub human: bool,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Subcommand)]
pub enum Command {
    /// Print the bundled agent operating skill without reading project or registry state.
    #[command(long_flag = "skill")]
    Skill,
    /// Write a minimal parcela.yaml for this directory, guessing the dev server command.
    Init {
        /// Project name; defaults to the main checkout directory name.
        #[arg(long)]
        name: Option<String>,
        /// Command that starts the web service on 127.0.0.1:$PORT.
        #[arg(long)]
        command: Option<String>,
    },
    /// Validate organizer schema (not backend compatibility). Does not create state.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Register and inspect existing checkouts. Never creates/deletes source or edits VCS history.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Print JSON context with planned URLs and explicit unsupported capabilities.
    Context,
    /// Check whether this machine and checkout are ready for parcela up. Changes nothing.
    Doctor {
        /// Caddy binary to probe instead of caddy on PATH.
        #[arg(long, hide = true)]
        caddy: Option<String>,
        /// Process Compose binary to probe instead of process-compose on PATH.
        #[arg(long, hide = true)]
        process_compose: Option<String>,
    },
    /// Start dedicated Caddy in the foreground. No install, sudo, trust or takeover.
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    /// Run a command in the selected checkout; register its identity on first use.
    Run {
        /// Preview name; otherwise reuse identity or infer it from the checkout.
        #[arg(long, conflicts_with_all = ["primary", "workspace"])]
        name: Option<String>,
        /// Explicitly use the project base URL, even from a linked checkout.
        #[arg(long, conflicts_with = "workspace")]
        primary: bool,
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        #[arg(last = true, required = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    /// Start the configured services; registration is automatic for this checkout.
    Up {
        /// Preview name; otherwise reuse identity or infer it from the checkout.
        #[arg(long, conflicts_with_all = ["primary", "workspace"])]
        name: Option<String>,
        /// Explicitly use the project base URL, even from a linked checkout.
        #[arg(long, conflicts_with = "workspace")]
        primary: bool,
        #[arg(long, default_value = "process-compose")]
        process_compose: String,
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        /// Keep an independent owner running after startup readiness.
        #[arg(long)]
        detach: bool,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Optionally run a named project command with environment context and a timeout.
    Task {
        name: String,
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Inspect live ownership and HTTP readiness; retain recorded state separately.
    Status {
        /// Explicit machine-wide runtime inventory.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
    },
    /// Stop through the live owner and preserve source/data. Never signal stored PIDs.
    #[command(visible_alias = "stop")]
    Down {
        /// Stop every running environment of this project.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
        /// Stop every running environment of every project.
        #[arg(long, conflicts_with_all = ["workspace", "all"])]
        all_projects: bool,
    },
    /// List every registered environment across projects.
    Ls,
    /// Forget a stopped environment and release its hostnames. Source and data stay on disk.
    Rm {
        /// Forget every stopped environment whose checkout directory no longer exists.
        #[arg(long, conflicts_with = "workspace")]
        prune: bool,
    },
    /// Recover control of a proven surviving native graph without replaying commands.
    Recover {
        #[arg(long)]
        detach: bool,
    },
    /// Stop the owned graph, then start the current manifest in a new generation.
    Restart {
        #[arg(long, default_value = "process-compose")]
        process_compose: String,
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        #[arg(long)]
        detach: bool,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Inspect services through the verified owner and Process Compose.
    Services,
    /// Print a bounded log tail for one service (default: the routed service).
    Logs {
        service: Option<String>,
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=10000))]
        tail: u32,
    },
    /// Print the reserved environment URL. Does not certify readiness.
    Url {
        #[arg(long)]
        service: Option<String>,
    },
}
#[derive(Subcommand)]
pub enum ProxyCommand {
    Run {
        #[arg(long, default_value = "caddy")]
        caddy: String,
        #[arg(long, default_value_t = 443)]
        https_port: u16,
        #[arg(long, default_value_t = 80)]
        http_port: u16,
        #[arg(long)]
        test_profile: bool,
        /// Explicit external 80/443 forwarding to these unprivileged listeners; no implicit fallback.
        #[arg(long)]
        forwarded_standard_ports: bool,
    },
    Status,
}
#[derive(Subcommand)]
pub enum ConfigCommand {
    Check,
}
#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// Discover sibling jj workspaces/Git worktrees without adopting or editing them.
    Discover,
    Adopt {
        #[arg(long, value_enum)]
        role: Role,
        #[arg(long)]
        label: String,
    },
    List {
        /// Explicit machine-wide inventory; otherwise scoped to the current project.
        #[arg(long)]
        all: bool,
    },
    Inspect,
}
