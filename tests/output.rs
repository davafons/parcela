use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const MANIFEST: &str = "version: 1\nproject: {id: output-test, name: demo}\ndefault_service: web\nservices: {web: {}}\nprocess_compose: {version: '0.5', processes: {web: {command: echo unused}}}\n";

fn cli(path: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(path)
        .arg("--state-dir")
        .arg(state)
        .args(args)
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}
fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn project() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let t = TempDir::new().unwrap();
    let path = t.path().canonicalize().unwrap().join("demo");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("parcela.yaml"), MANIFEST).unwrap();
    let state = t.path().join("state");
    (t, path, state)
}

#[test]
fn piped_stdout_defaults_to_json_and_json_flag_forces_it() {
    let (_t, path, state) = project();
    for args in [&["config", "check"][..], &["--json", "config", "check"]] {
        let value: serde_json::Value =
            serde_json::from_str(&stdout(&cli(&path, &state, args))).unwrap();
        assert_eq!(value["valid"], true);
        assert_eq!(value["project_id"], "output-test");
    }
    let forced: serde_json::Value = serde_json::from_str(&stdout(&cli(
        &path,
        &state,
        &["--human", "--json", "status", "--all"],
    )))
    .unwrap();
    assert_eq!(
        forced,
        serde_json::json!({"runtime_verified": false, "workspaces": []})
    );
}

#[test]
fn human_output_is_aligned_text_without_color_when_piped() {
    let (_t, path, state) = project();
    assert_eq!(
        stdout(&cli(&path, &state, &["--human", "config", "check"])),
        format!(
            "✓ {} is valid\n  project   output-test\n",
            path.join("parcela.yaml").display()
        )
    );
    assert_eq!(
        stdout(&cli(&path, &state, &["--human", "status", "--all"])),
        "No environments registered. Start one with parcela up.\n"
    );
    stdout(&cli(
        &path,
        &state,
        &[
            "workspace",
            "adopt",
            "--role",
            "primary",
            "--label",
            "primary",
        ],
    ));
    assert_eq!(
        stdout(&cli(&path, &state, &["--human", "status", "--all"])),
        format!(
            "PROJECT      NAME     STATE    URL                     PATH\noutput-test  primary  stopped  https://demo.localhost  {}\n",
            path.display()
        )
    );
    assert_eq!(
        stdout(&cli(&path, &state, &["--human", "down"])),
        "✓ output-test was already stopped\n"
    );
}
