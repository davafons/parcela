use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

mod common;

fn cli(path: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(path)
        .arg("--state-dir")
        .arg(state)
        .args(args)
        .output()
        .unwrap()
}
fn up(path: &Path, state: &Path, flags: &[&str]) -> String {
    let mut args = vec![
        "up",
        // Registers the checkout, then preflight refuses the proxy's port before anything starts.
        "--backend-port",
        "1443",
    ];
    args.extend_from_slice(flags);
    let output = cli(path, state, &args);
    assert_eq!(output.status.code(), Some(1));
    String::from_utf8(output.stderr).unwrap()
}
fn checkout(root: &Path, name: &str, manifest: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("parcela.yaml"), manifest).unwrap();
    path
}

#[test]
fn registration_conflicts_name_the_holder_and_the_fix() {
    let t = TempDir::new().unwrap();
    let root = t.path().canonicalize().unwrap();
    let state = root.join("state");
    let manifest = "version: 1\nproject: demo\nservices:\n  web: {command: bin/web, http: {healthcheck: /}}\n  api: {command: bin/api, http: {subdomain: api, healthcheck: /}}\n";
    let first = checkout(&root, "first", manifest);
    let second = checkout(&root, "second", manifest);
    let third = checkout(&root, "third", manifest);
    up(&first, &state, &[]);
    assert_eq!(
        up(&second, &state, &[]),
        format!(
            "parcela: demo already has a primary checkout at {}; start this one as a preview with --name <name>\n",
            first.display()
        )
    );
    up(&second, &state, &["--name", "review"]);
    assert_eq!(
        up(&third, &state, &["--name", "review"]),
        format!(
            "parcela: preview name review is already used by {}; choose another with --name <name>\n",
            second.display()
        )
    );
    assert_eq!(
        up(&third, &state, &["--name", "api"]),
        format!(
            "parcela: api-demo.localhost is already used by {}; choose another preview name with --name <name>\n",
            first.display()
        )
    );
}

#[test]
fn missing_manifest_and_unstarted_checkout_point_to_the_next_command() {
    let t = TempDir::new().unwrap();
    let root = t.path().canonicalize().unwrap();
    let state = root.join("state");
    let output = cli(&root, &state, &["status"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "parcela: nothing is registered yet; start this checkout with `parcela up`\n"
    );
    let output = cli(&root, &state, &["up"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "parcela: no parcela.yaml in {} or above; create one with `parcela init`, or pass --path to a configured checkout\n",
            root.display()
        )
    );
    let mut bare = Command::new(env!("CARGO_BIN_EXE_parcela"));
    bare.arg("--path").arg(&root).arg("--state-dir").arg(&state);
    assert_eq!(
        common::on_terminal(bare).1,
        "No parcela.yaml here. Create one with `parcela init`, then run `parcela up`.\nSee `parcela --help` for every command.\n"
    );
    let app = checkout(
        &root,
        "app",
        "version: 1\nproject: app\nservices: {web: {command: bin/web, http: {healthcheck: /}}}\n",
    );
    let bare = cli(&app, &state, &[]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bare.stdout).unwrap(),
        serde_json::json!({"hint": "Not started yet. Run `parcela up`."})
    );
    up(&app, &state, &[]);
    let bare = cli(&app, &state, &[]);
    let status: serde_json::Value = serde_json::from_slice(&bare.stdout).unwrap();
    assert_eq!(status["state"], "stopped");
}
