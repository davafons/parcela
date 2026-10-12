use parcela::{
    config::Manifest,
    discovery,
    registry::{Registry, Role},
};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const MANIFEST: &str =
    "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}}\n";
fn output(command: &mut Command) -> Output {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
}
fn jj(source: &Path, args: &[&str]) -> String {
    String::from_utf8(
        output(
            Command::new("jj")
                .arg("--repository")
                .arg(source)
                .args(["--config", "signing.behavior=\"drop\""])
                .args(args),
        )
        .stdout,
    )
    .unwrap()
}
fn git(source: &Path, args: &[&str]) -> String {
    assert!(
        !source.join(".jj").exists(),
        "never use Git in a colocated fixture"
    );
    String::from_utf8(output(Command::new("git").arg("-C").arg(source).args(args)).stdout).unwrap()
}
#[test]
fn jj_discovery_prefers_jj_and_does_not_snapshot_or_adopt_siblings() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    output(
        Command::new("jj")
            .args(["git", "init", "--colocate"])
            .arg(&source),
    );
    fs::write(source.join("parcela.yaml"), MANIFEST).unwrap();
    jj(&source, &["bookmark", "set", "main"]);
    jj(&source, &["new", "main"]);
    let sibling = temp.path().join("sibling with spaces");
    jj(
        &source,
        &[
            "workspace",
            "add",
            "--name",
            "preview",
            "-r",
            "main",
            sibling.to_str().unwrap(),
        ],
    );
    fs::write(
        source.join("unsnapshotted.txt"),
        "keep this edit outside jj's operation log",
    )
    .unwrap();
    let operation = jj(
        &source,
        &[
            "--ignore-working-copy",
            "--at-op=@",
            "op",
            "log",
            "--limit",
            "1",
            "--template",
            "id",
        ],
    );
    let state = temp.path().join("state");
    let mut registry = Registry::open(&state, true).unwrap().unwrap();
    let manifest = Manifest::load(&source.join("parcela.yaml")).unwrap();
    let adopted = registry
        .adopt(&manifest, &source, Role::Primary, "main")
        .unwrap();
    let rows = discovery::discover(&source, Some(&registry)).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.vcs == "jj"));
    assert_eq!(
        rows.iter()
            .find(|row| row.name.as_deref() == Some("default"))
            .unwrap()
            .adopted_environment_id,
        Some(adopted.id)
    );
    assert!(
        rows.iter()
            .find(|row| row.name.as_deref() == Some("preview"))
            .unwrap()
            .adopted_environment_id
            .is_none()
    );
    assert_eq!(registry.list(None).unwrap().len(), 1);
    assert_eq!(
        operation,
        jj(
            &source,
            &[
                "--ignore-working-copy",
                "--at-op=@",
                "op",
                "log",
                "--limit",
                "1",
                "--template",
                "id"
            ]
        )
    );
}
#[test]
fn git_discovery_lists_detached_worktrees_without_changing_history() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch", "main"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Parcela Tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    let sibling = temp.path().join("detached with spaces");
    git(
        &source,
        &["worktree", "add", "--detach", sibling.to_str().unwrap()],
    );
    let head = git(&source, &["rev-parse", "HEAD"]);
    let rows = discovery::discover(&source, None).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.vcs == "git"));
    assert_eq!(
        rows.iter()
            .find(|row| row.source == source.canonicalize().unwrap())
            .unwrap()
            .refs,
        ["main"]
    );
    assert!(
        rows.iter()
            .find(|row| row.source == sibling.canonicalize().unwrap())
            .unwrap()
            .refs
            .is_empty()
    );
    assert_eq!(head, git(&source, &["rev-parse", "HEAD"]));
}
#[test]
fn plain_discovery_and_global_status_do_not_create_state() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("absent-state");
    for args in [["workspace", "discover"], ["status", "--all"]] {
        output(
            Command::new(env!("CARGO_BIN_EXE_parcela"))
                .arg("--path")
                .arg(temp.path())
                .arg("--state-dir")
                .arg(&state)
                .args(args),
        );
        assert!(!state.exists());
    }
    let rows = discovery::discover(temp.path(), None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].vcs, "plain");
    assert!(!rows[0].manifest_exists);
}
