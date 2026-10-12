//! Read-only VCS inventory. Discovery never adopts, creates, snapshots, or removes a checkout.
use crate::{process, registry::Registry};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Debug, Serialize)]
pub struct Checkout {
    pub source: PathBuf,
    pub name: Option<String>,
    pub vcs: String,
    pub revision: Option<String>,
    pub refs: Vec<String>,
    pub adopted_environment_id: Option<String>,
    pub source_exists: bool,
    pub manifest_exists: bool,
}
fn checkout(
    source: PathBuf,
    name: Option<String>,
    vcs: &str,
    revision: Option<String>,
    refs: Vec<String>,
) -> Checkout {
    Checkout {
        source_exists: source.is_dir(),
        manifest_exists: source.join("parcela.yaml").is_file(),
        source,
        name,
        vcs: vcs.into(),
        revision,
        refs,
        adopted_environment_id: None,
    }
}
pub fn discover(source: &Path, registry: Option<&Registry>) -> Result<Vec<Checkout>> {
    let source = source
        .canonicalize()
        .context("discovery requires an existing source directory")?;
    ensure!(source.is_dir(), "discovery requires a directory");
    let root = source
        .ancestors()
        .find(|path| path.join(".jj").is_dir() || path.join(".git").exists());
    let mut rows = if let Some(root) = root {
        let jj = root.join(".jj").is_dir();
        let mut command = Command::new(if jj { "jj" } else { "git" });
        if jj {
            command.arg("--repository").arg(root).args(["--ignore-working-copy", "--at-op=@", "--color=never", "--no-pager", "workspace", "list", "--template",
                "name ++ \"\\0\" ++ root ++ \"\\0\" ++ target.commit_id() ++ \"\\0\" ++ json(target.local_bookmarks().map(|ref| ref.name())) ++ \"\\0\""]);
        } else {
            command
                .arg("-C")
                .arg(root)
                .args(["worktree", "list", "--porcelain", "-z"]);
        }
        let output = process::capture(&mut command, Duration::from_secs(3), 512000)?;
        ensure!(
            output.status.success(),
            "{} discovery unavailable (jj needs workspace root templates, tested with 0.40): {}",
            if jj { "jj" } else { "Git" },
            String::from_utf8_lossy(&output.stderr)
        );
        if jj {
            parse_jj(&output.stdout)?
        } else {
            parse_git(&output.stdout)?
        }
    } else {
        vec![checkout(source, None, "plain", None, Vec::new())]
    };
    if let Some(registry) = registry {
        let adopted = registry.list(None)?;
        for row in &mut rows {
            let canonical = row
                .source
                .canonicalize()
                .unwrap_or_else(|_| row.source.clone());
            row.adopted_environment_id = adopted
                .iter()
                .find(|workspace| workspace.source == canonical)
                .map(|workspace| workspace.id.clone());
        }
    }
    Ok(rows)
}
fn parse_jj(bytes: &[u8]) -> Result<Vec<Checkout>> {
    let text = std::str::from_utf8(bytes)?;
    let fields = text.split_terminator('\0').collect::<Vec<_>>();
    ensure!(fields.len() % 4 == 0, "invalid jj workspace inventory");
    fields
        .chunks_exact(4)
        .map(|row| {
            let path = PathBuf::from(row[1]);
            ensure!(
                path.is_absolute(),
                "jj returned a nonabsolute workspace root"
            );
            Ok(checkout(
                path,
                Some(row[0].into()),
                "jj",
                Some(row[2].into()),
                serde_json::from_str(row[3])?,
            ))
        })
        .collect()
}
fn parse_git(bytes: &[u8]) -> Result<Vec<Checkout>> {
    let mut rows = Vec::new();
    let mut current: Option<Checkout> = None;
    for field in std::str::from_utf8(bytes)?.split('\0') {
        if let Some(path) = field.strip_prefix("worktree ") {
            ensure!(current.is_none(), "invalid Git worktree record");
            let path = PathBuf::from(path);
            ensure!(
                path.is_absolute(),
                "Git returned a nonabsolute worktree root"
            );
            current = Some(checkout(path, None, "git", None, Vec::new()));
        } else if field.is_empty() {
            if let Some(row) = current.take() {
                rows.push(row);
            }
        } else if let Some(row) = &mut current {
            if let Some(revision) = field.strip_prefix("HEAD ") {
                row.revision = Some(revision.into());
            }
            if let Some(branch) = field.strip_prefix("branch ") {
                row.refs
                    .push(branch.strip_prefix("refs/heads/").unwrap_or(branch).into());
            }
        }
    }
    ensure!(current.is_none(), "incomplete Git worktree inventory");
    Ok(rows)
}
#[cfg(test)]
mod tests {
    #[test]
    fn nul_inventory_preserves_paths_and_detached_state() {
        let rows = super::parse_git(b"worktree /tmp/with\nnewline\0HEAD abc\0branch refs/heads/main\0\0worktree /tmp/detached\0HEAD def\0detached\0\0").unwrap();
        assert_eq!(rows[0].source.to_str().unwrap(), "/tmp/with\nnewline");
        assert_eq!(rows[0].refs, ["main"]);
        assert!(rows[1].refs.is_empty());
        assert!(super::parse_jj(b"missing\0fields\0").is_err());
    }
}
