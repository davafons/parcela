use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const HEADER: &str = "# Parcela runs each service at its own HTTPS URL. Reference: https://github.com/davafons/parcela/blob/main/docs/configuration.md\n# Services must listen on 127.0.0.1:$PORT.\n";

fn cli(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(path)
        .arg("--state-dir")
        .arg(path.join("unused-state"))
        .env_remove("PARCELA_HTTPS_PORT")
        .args(args)
        .output()
        .unwrap()
}
fn init(path: &Path, args: &[&str]) -> String {
    let mut argv = vec!["init"];
    argv.extend_from_slice(args);
    let output = cli(path, &argv);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let check = cli(path, &["config", "check"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(!path.join("unused-state").exists());
    fs::read_to_string(path.join("parcela.yaml")).unwrap()
}
fn directory(t: &TempDir, name: &str) -> std::path::PathBuf {
    let path = t.path().canonicalize().unwrap().join(name);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn init_writes_a_valid_one_line_manifest_named_after_the_directory() {
    let t = TempDir::new().unwrap();
    let path = directory(&t, "Notes App");
    let output = cli(&path, &["--json", "init"]);
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result,
        serde_json::json!({
            "created": path.join("parcela.yaml"),
            "project": "notes-app",
            "url": "https://notes-app.localhost:1443",
            "services": {"web": "python3 -m http.server $PORT --bind 127.0.0.1"}
        })
    );
    assert_eq!(
        fs::read_to_string(path.join("parcela.yaml")).unwrap(),
        format!(
            "{HEADER}# No dev server detected; this serves the directory, replace it with yours\nweb: python3 -m http.server $PORT --bind 127.0.0.1\n"
        )
    );
    let check: serde_json::Value =
        serde_json::from_slice(&cli(&path, &["config", "check"]).stdout).unwrap();
    assert_eq!(check["project_id"], "notes-app");
}

#[test]
fn init_prints_the_url_on_the_port_from_parcela_https_port() {
    let t = TempDir::new().unwrap();
    let path = directory(&t, "notes");
    let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
        .arg("--path")
        .arg(&path)
        .arg("--state-dir")
        .arg(path.join("unused-state"))
        .env("PARCELA_HTTPS_PORT", "4433")
        .args(["--json", "init"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["url"], "https://notes.localhost:4433");
    assert!(!path.join("unused-state").exists());
}

#[test]
fn init_suggests_the_command_from_project_files() {
    let t = TempDir::new().unwrap();
    let node = directory(&t, "node");
    fs::write(node.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
    assert_eq!(
        init(&node, &[]),
        format!(
            "{HEADER}# package.json has a dev script; check that it accepts these flags\nweb: npm run dev -- --port $PORT --host 127.0.0.1\n"
        )
    );
    let rails = directory(&t, "rails");
    fs::write(rails.join("Gemfile"), "").unwrap();
    fs::create_dir(rails.join("bin")).unwrap();
    fs::write(rails.join("bin/rails"), "").unwrap();
    assert_eq!(
        init(&rails, &["--name", "Shop Front"]),
        format!(
            "{HEADER}project: shop-front\n# Rails app detected\nweb: bin/rails server -p $PORT -b 127.0.0.1\n"
        )
    );
    let custom = directory(&t, "custom");
    assert_eq!(
        init(&custom, &["--command", "bin/dev: --port $PORT"]),
        format!("{HEADER}web: 'bin/dev: --port $PORT'\n")
    );
}

#[test]
fn init_never_overwrites_an_existing_manifest() {
    let t = TempDir::new().unwrap();
    let path = directory(&t, "kept");
    fs::write(path.join("parcela.yaml"), "web: mine\n").unwrap();
    let output = cli(&path, &["init"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "parcela: {} already exists; edit it, or check it with parcela config check\n",
            path.join("parcela.yaml").display()
        )
    );
    assert_eq!(
        fs::read_to_string(path.join("parcela.yaml")).unwrap(),
        "web: mine\n"
    );
}
