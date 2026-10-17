use std::{fs, process::Command};
use tempfile::TempDir;

fn check(manifest: &str) -> (Option<i32>, String) {
    let t = TempDir::new().unwrap();
    let path = t.path().canonicalize().unwrap().join("app");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("parcela.yaml"), manifest).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(&path)
        .args(["config", "check"])
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn config_check_requires_a_process_for_every_http_service() {
    let head = "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}, api: {}}\n";
    for (graph, expected) in [
        (
            "",
            "parcela: nothing to start; add a command for the web service, like `web: bin/dev`, or a process_compose.processes map\n",
        ),
        (
            "process_compose: {version: '0.5'}\n",
            "parcela: process_compose has no processes; add one named after each service\n",
        ),
        (
            "process_compose: {processes: {}}\n",
            "parcela: process_compose has no processes; add one named after each service\n",
        ),
        (
            "process_compose: {processes: {web: {command: bin/web}, worker: {command: bin/jobs}}}\n",
            "parcela: service api has no process to start; add process_compose.processes.api with its command\n",
        ),
    ] {
        assert_eq!(
            check(&format!("{head}{graph}")),
            (Some(1), expected.to_owned()),
            "{graph}"
        );
    }
    assert_eq!(
        check(&format!(
            "{head}process_compose: {{processes: {{web: {{command: a}}, api: {{command: b}}}}}}\n"
        )),
        (Some(0), String::new())
    );
}

#[test]
fn every_bundled_example_passes_config_check() {
    for example in ["generic", "docs-demo"] {
        let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
            .arg("--path")
            .arg(format!("{}/examples/{example}", env!("CARGO_MANIFEST_DIR")))
            .args(["config", "check"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{example}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
