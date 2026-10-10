use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use parcela::{
    config::{Manifest, locate},
    context::ResolvedContext,
    registry::{Registry, Role},
};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "Workspace organizer (Rust foundation; runtime not yet implemented)"
)]
struct Cli {
    /// Checkout directory; defaults to the current directory.
    #[arg(long, global = true, default_value = ".")]
    path: PathBuf,
    /// Private registry directory; defaults to PARCELA_STATE_DIR, XDG_STATE_HOME/parcela or ~/.local/state/parcela.
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
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
    /// Report implemented capabilities, without launching backends or changing the host.
    Doctor,
}
#[derive(Subcommand)]
enum ConfigCommand {
    Check,
}
#[derive(Subcommand)]
enum WorkspaceCommand {
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

fn state_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = if let Some(path) = explicit {
        path
    } else if let Some(path) = std::env::var_os("PARCELA_STATE_DIR") {
        path.into()
    } else if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(path);
        ensure!(path.is_absolute(), "XDG_STATE_HOME must be absolute");
        path.join("parcela")
    } else {
        PathBuf::from(std::env::var_os("HOME").context("HOME missing; set --state-dir")?)
            .join(".local/state/parcela")
    };
    ensure!(
        !path.as_os_str().is_empty(),
        "state directory must not be empty"
    );
    Ok(path)
}
fn project(path: &Path) -> Result<(PathBuf, Manifest)> {
    let source = locate(path)?;
    let manifest = Manifest::load(&source.join("parcela.yaml"))?;
    Ok((source, manifest))
}
fn json(value: &impl serde::Serialize) -> Result<()> {
    // stdout is reserved for JSON; diagnostics go to stderr.
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            let (source, manifest) = project(&cli.path)?;
            json(
                &serde_json::json!({"valid": true, "project_id": manifest.project.id, "manifest": source.join("parcela.yaml"),
                "backend_graph_validated": false, "schema_version": 1}),
            )
        }
        Command::Doctor => json(&serde_json::json!({
            "implementation": "rust", "backends": {"http_tls": "managed Caddy", "supervision": "managed Process Compose"},
            "supported": ["config check", "workspace adopt", "workspace list", "workspace inspect", "context"],
            "unsupported": ["endpoint allocation", "run", "up", "stop", "tasks", "TLS/trust", "sharing", "boot service"],
            "backend_versions_verified": false, "host_modified": false
        })),
        Command::Workspace {
            command: WorkspaceCommand::List { all },
        } => {
            let manifest = if all {
                None
            } else {
                Some(project(&cli.path)?.1)
            };
            let state = state_dir(cli.state_dir)?;
            let rows = match Registry::open(&state, false)? {
                Some(registry) => {
                    registry.list(manifest.as_ref().map(|m| m.project.id.as_str()))?
                }
                None => Vec::new(),
            };
            let inventory: Vec<_> = rows.into_iter().map(|workspace| {
                serde_json::json!({"source_exists": workspace.source.is_dir(), "workspace": workspace})
            }).collect();
            json(&serde_json::json!({"workspaces": inventory, "runtime_verified": false}))
        }
        Command::Workspace {
            command: WorkspaceCommand::Adopt { role, label },
        } => {
            let (source, manifest) = project(&cli.path)?;
            // Fail invalid names before even creating registry state.
            manifest.hostname(role == Role::Primary, &label)?;
            let state = state_dir(cli.state_dir)?;
            let mut registry = Registry::open(&state, true)?.context("registry not created")?;
            let workspace = registry.adopt(&manifest, &source, role, &label)?;
            json(&ResolvedContext::new(&registry, &manifest, workspace))
        }
        Command::Context
        | Command::Workspace {
            command: WorkspaceCommand::Inspect,
        } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let Some(registry) = Registry::open(&state, false)? else {
                bail!("no registry; run workspace adopt with explicit --role and --label");
            };
            let workspace = registry.current(&source, &manifest)?;
            json(&ResolvedContext::new(&registry, &manifest, workspace))
        }
    }
}
fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("parcela: {error:#}");
        std::process::exit(1);
    }
}
