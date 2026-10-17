use parcela::config::Manifest;
use serde_json::json;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn load(directory: &Path, text: &str) -> anyhow::Result<Manifest> {
    fs::create_dir_all(directory).unwrap();
    fs::write(directory.join("parcela.yaml"), text).unwrap();
    Manifest::load(&directory.join("parcela.yaml"))
}
fn error(directory: &Path, text: &str) -> String {
    load(directory, text).unwrap_err().root_cause().to_string()
}
fn run(path: &Path, program: &str, args: &[&str]) {
    let output = Command::new(program)
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn one_line_manifest_becomes_a_web_service_named_after_the_checkout() {
    let t = TempDir::new().unwrap();
    let manifest = load(
        &t.path().join("My Notes"),
        "web: npm run dev -- --port $PORT --host 127.0.0.1\n",
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&manifest).unwrap(),
        json!({
            "version": 1,
            "project": {"id": "my-notes", "name": "my-notes"},
            "default_service": "web",
            "services": {"web": {"subdomain": null, "readiness_path": "/"}},
            "tasks": {},
            "process_compose": {"version": "0.5", "processes": {
                "web": {"command": "npm run dev -- --port $PORT --host 127.0.0.1"}
            }},
            "environment": {},
            "inherit_environment": true,
            "shutdown_timeout_seconds": 300
        })
    );
}

#[test]
fn each_short_form_service_gets_its_own_port_and_workers_get_no_route() {
    let t = TempDir::new().unwrap();
    let manifest = load(
        &t.path().join("shop"),
        "project: {name: store}\nservices:\n  web: bin/rails server -p $PORT\n  admin-api: {command: 'node api.js --port ${PORT} $PORTAL', subdomain: api, readiness_path: /up}\nworkers:\n  jobs: bin/jobs\n",
    )
    .unwrap();
    let value = serde_json::to_value(&manifest).unwrap();
    assert_eq!(value["project"], json!({"id": "store", "name": "store"}));
    assert_eq!(
        value["services"],
        json!({"admin-api": {"subdomain": "api", "readiness_path": "/up"}, "web": {"subdomain": null, "readiness_path": "/"}})
    );
    assert_eq!(
        value["process_compose"]["processes"],
        json!({
            "web": {"command": "bin/rails server -p $PORT"},
            "admin-api": {
                "command": "node api.js --port ${PARCELA_SERVICE_ADMIN_API_PORT} $PORTAL",
                "environment": ["PORT=${PARCELA_SERVICE_ADMIN_API_PORT}"]
            },
            "jobs": {"command": "bin/jobs"}
        })
    );
}

#[test]
fn full_form_is_unchanged_and_ambiguous_forms_are_refused() {
    let t = TempDir::new().unwrap();
    let full = "version: 1\nproject: {id: a.b, name: demo}\ndefault_service: web\nservices: {web: {}}\nprocess_compose: {version: '0.5', processes: {web: {command: x}}}\n";
    let manifest = load(&t.path().join("full"), full).unwrap();
    assert_eq!(manifest.project.id, "a.b");
    assert_eq!(
        manifest.process_compose,
        Some(json!({"version": "0.5", "processes": {"web": {"command": "x"}}}))
    );
    let dir = t.path().join("bad");
    for (text, expected) in [
        (
            "web: x\nservices: {api: bin/api}\n",
            "declare services either at the top level (`web: bin/dev`) or under `services:`, not both",
        ),
        (
            "web: x\nprocess_compose: {processes: {}}\n",
            "the short form generates process_compose from service commands; remove process_compose or use the full form for every service",
        ),
        (
            "defualt_service: web\nweb: x\n",
            "unknown field \"defualt_service\"; did you mean \"default_service\"?",
        ),
        (
            "worker: bin/jobs\n",
            "unknown field \"worker\"; did you mean \"workers\"?",
        ),
        (
            "api: a\nadmin: b\n",
            "several services and none is named web; set default_service to the one that gets the base URL",
        ),
        (
            "web: {port: 3}\n",
            "unknown field \"web\"; a top-level service is a command string, like `web: bin/dev`",
        ),
        (
            "web: x\nworkers: {web: bin/jobs}\n",
            "worker web has the same name as a service",
        ),
    ] {
        assert_eq!(error(&dir, text), expected, "{text}");
    }
}

#[test]
fn linked_checkouts_share_the_main_checkout_project_name() {
    let t = TempDir::new().unwrap();
    let main = t.path().join("Main-Repo");
    fs::create_dir(&main).unwrap();
    run(&main, "jj", &["git", "init", "--quiet"]);
    run(
        &main,
        "jj",
        &["workspace", "add", "--quiet", "../linked-jj"],
    );
    run(&main, "git", &["init", "--quiet", "../git-main"]);
    let git = t.path().join("git-main");
    run(
        &git,
        "git",
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    run(
        &git,
        "git",
        &["worktree", "add", "--quiet", "../linked-git"],
    );
    for (directory, expected) in [
        ("Main-Repo", "main-repo"),
        ("linked-jj", "main-repo"),
        ("git-main", "git-main"),
        ("linked-git", "git-main"),
    ] {
        let manifest = load(&t.path().join(directory), "web: x\n").unwrap();
        assert_eq!(manifest.project.name, expected, "{directory}");
    }
}
