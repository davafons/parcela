use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use tempfile::TempDir;

/// A port nothing listens on, so the stopped proxy reads as startable.
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn doctor(path: &Path, state: &Path, port: u16) -> (Option<i32>, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(path)
        .arg("--state-dir")
        .arg(state)
        .env("PARCELA_HTTPS_PORT", port.to_string())
        .args([
            "doctor",
            "--caddy",
            "/nonexistent/caddy",
            "--process-compose",
            "/nonexistent/process-compose",
        ])
        .output()
        .unwrap();
    (
        output.status.code(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}
fn check<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == name)
        .unwrap()
}

#[test]
fn doctor_reports_a_stable_checklist_with_fixes_and_changes_nothing() {
    let t = TempDir::new().unwrap();
    let path = t.path().canonicalize().unwrap();
    let state = path.join("state");
    let port = free_port();
    let (code, report) = doctor(&path, &state, port);
    assert_eq!(code, Some(1));
    assert_eq!(report["ready"], false);
    let names: Vec<&str> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "process-compose",
            "caddy",
            "lsof",
            "ps",
            "config",
            "state",
            "proxy"
        ]
    );
    for check in report["checks"].as_array().unwrap() {
        let keys: Vec<&String> = check.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["detail", "fix", "name", "ok"]);
        assert_eq!(check["ok"] == false, check["fix"].is_string(), "{check}");
    }
    assert_eq!(
        check(&report, "process-compose"),
        &json!({"name": "process-compose", "ok": false, "detail": "/nonexistent/process-compose not found on PATH", "fix": "install Process Compose 1.x: brew install f1bonacc1/tap/process-compose (or https://f1bonacc1.github.io/process-compose/installation/)"})
    );
    assert_eq!(
        check(&report, "config"),
        &json!({"name": "config", "ok": false, "detail": format!("no parcela.yaml in {} or above; create one with `parcela init`, or pass --path to a configured checkout", path.display()), "fix": "parcela init"})
    );
    assert_eq!(
        check(&report, "state"),
        &json!({"name": "state", "ok": true, "detail": format!("{} will be created on first parcela up", state.display()), "fix": null})
    );
    assert_eq!(
        check(&report, "proxy"),
        &json!({"name": "proxy", "ok": true, "detail": format!("not running; `parcela up` starts it on HTTPS on :{port}"), "fix": null})
    );
    assert!(!state.exists());

    fs::write(
        path.join("parcela.yaml"),
        "version: 1\nproject: app\nservices: {web: {command: bin/dev, http: {healthcheck: /}}}\n",
    )
    .unwrap();
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    let (_, report) = doctor(&path, &state, port);
    assert_eq!(check(&report, "config")["ok"], true);
    assert_eq!(
        check(&report, "state")["fix"],
        format!("chmod 700 {}", state.display())
    );
    assert_eq!(fs::read_dir(&state).unwrap().count(), 0);
}
