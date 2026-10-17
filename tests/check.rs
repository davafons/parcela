use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn parcela(path: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(path)
        .arg("--state-dir")
        .arg(path.join("state"))
        .args(args)
        .output()
        .unwrap()
}

/// The doctor's config check as `{ok, detail}`.
fn doctor_config(path: &Path) -> (bool, String) {
    let report: Value =
        serde_json::from_slice(&parcela(path, &["--json", "doctor"]).stdout).unwrap();
    let check = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "config")
        .unwrap();
    (
        check["ok"].as_bool().unwrap(),
        check["detail"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn up_and_doctor_report_the_same_error_for_a_service_without_a_process() {
    let head = "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}, api: {}}\n";
    for (graph, expected) in [
        (
            "",
            "nothing to start; add a command for the web service, like `web: bin/dev`, or a process_compose.processes map",
        ),
        (
            "process_compose: {version: '0.5'}\n",
            "process_compose has no processes; add one named after each service",
        ),
        (
            "process_compose: {processes: {}}\n",
            "process_compose has no processes; add one named after each service",
        ),
        (
            "process_compose: {processes: {web: {command: bin/web}, worker: {command: bin/jobs}}}\n",
            "service api has no process to start; add process_compose.processes.api with its command",
        ),
    ] {
        let t = TempDir::new().unwrap();
        let path = t.path().canonicalize().unwrap();
        fs::write(path.join("parcela.yaml"), format!("{head}{graph}")).unwrap();
        let up = parcela(
            &path,
            &["up", "--process-compose", "/nonexistent/process-compose"],
        );
        assert_eq!(up.status.code(), Some(1), "{graph}");
        assert_eq!(
            String::from_utf8(up.stderr).unwrap(),
            format!("parcela: {expected}\n"),
            "{graph}"
        );
        assert_eq!(
            doctor_config(&path),
            (false, expected.to_owned()),
            "{graph}"
        );
        assert!(!path.join("state").exists(), "{graph}");
    }
}

#[test]
fn a_single_service_may_name_its_process_differently() {
    let t = TempDir::new().unwrap();
    let path = t.path().canonicalize().unwrap();
    fs::write(
        path.join("parcela.yaml"),
        "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}}\nprocess_compose: {processes: {server: {command: bin/web}}}\n",
    )
    .unwrap();
    assert!(doctor_config(&path).0);
}

#[test]
fn every_bundled_example_passes_the_doctor_config_check() {
    for example in ["generic", "docs-demo"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(example);
        let report: Value = serde_json::from_slice(
            &Command::new(env!("CARGO_BIN_EXE_parcela"))
                .arg("--path")
                .arg(&path)
                .args([
                    "--state-dir",
                    "/nonexistent/parcela-state",
                    "--json",
                    "doctor",
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let config = report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == "config")
            .unwrap();
        assert_eq!(config["ok"], true, "{example}: {config}");
    }
}
