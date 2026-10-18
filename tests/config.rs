use parcela::{config::Manifest, registry::Endpoint, runtime::compose_graph};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::TempDir;

const SHOP: &str = "\
version: 1
project: shop
env:
  DATABASE_URL: postgres://127.0.0.1:${PARCELA_PORT_DB}/shop
ports: [db]
services:
  db:
    command: postgres -D \"$PARCELA_DATA_DIR/postgres\" -p \"$PARCELA_PORT_DB\"
    healthcheck: pg_isready -h 127.0.0.1 -p \"$PARCELA_PORT_DB\"
  web:
    command: bin/rails server -p $PORT
    http:
      healthcheck: /up
    depends_on: [db]
  api:
    command: node api.js
    working_dir: api
    env:
      API_NAME: api-${PARCELA_ENVIRONMENT_NAME}
    http:
      subdomain: api
      healthcheck: /health
  jobs:
    command: bin/jobs
    depends_on: [db]
hooks:
  setup:
    command: bin/rails db:setup
    depends_on: [db]
  prestart:
    command: bin/rails db:migrate
    depends_on: [db]
tasks:
  test:
    command: bin/rails test
    timeout_seconds: 600
x-notes: anything
";

fn error(text: &str) -> String {
    format!("{:#}", Manifest::parse(text).unwrap_err())
}

#[test]
fn a_file_with_every_field_becomes_the_runtime_shape() {
    let manifest = Manifest::parse(SHOP).unwrap();
    let value = serde_json::to_value(&manifest).unwrap();
    assert_eq!(value["project"], json!({"id": "shop", "name": "shop"}));
    assert_eq!(value["default_service"], "web");
    assert_eq!(
        value["services"],
        json!({
            "api": {"subdomain": "api", "readiness_path": "/health"},
            "web": {"subdomain": null, "readiness_path": "/up"},
        })
    );
    assert_eq!(
        value["processes"]["db"]["healthcheck"],
        "pg_isready -h 127.0.0.1 -p \"$PARCELA_PORT_DB\""
    );
    assert_eq!(value["processes"]["api"]["working_dir"], "api");
    assert_eq!(value["hooks"]["setup"]["depends_on"], json!(["db"]));
    assert_eq!(value["tasks"]["test"]["command"], "bin/rails test");
    assert_eq!(value["ports"], json!(["db"]));
}

#[test]
fn hooks_run_after_their_dependencies_and_before_everything_else() {
    let manifest = Manifest::parse(SHOP).unwrap();
    let endpoints = [("web", 21000), ("api", 21001)].map(|(service, port)| Endpoint {
        service: service.into(),
        hostname: format!("{service}.localhost"),
        backend_port: port,
        public_url: format!("https://{service}.localhost:1443"),
    });
    let env = BTreeMap::from([("PARCELA_ENVIRONMENT_NAME".to_owned(), "signup".to_owned())]);
    let graph = compose_graph(&manifest, &endpoints, &env, Path::new("/state/env"), true).unwrap();
    let processes = &graph["processes"];
    assert_eq!(graph["disable_env_expansion"], true);
    assert_eq!(
        processes["setup"],
        json!({"command": "(\nbin/rails db:setup\n)\nstatus=$?\nprintf %s \"$status\" > '/state/env/hooks/setup.exit'\nexit \"$status\"",
            "availability": {"restart": "no"},
            "depends_on": {"db": {"condition": "process_healthy"}}})
    );
    assert_eq!(
        processes["prestart"]["depends_on"],
        json!({"db": {"condition": "process_healthy"}, "setup": {"condition": "process_completed_successfully"}})
    );
    assert_eq!(processes["db"]["depends_on"], json!({}));
    assert_eq!(
        processes["web"]["depends_on"],
        json!({"db": {"condition": "process_healthy"}, "prestart": {"condition": "process_completed_successfully"}})
    );
    assert_eq!(processes["web"]["environment"], json!(["PORT=21000"]));
    assert_eq!(
        processes["web"]["readiness_probe"]["http_get"],
        json!({"host": "127.0.0.1", "scheme": "http", "port": 21000, "path": "/up"})
    );
    assert_eq!(
        processes["api"]["environment"],
        json!(["PORT=21001", "API_NAME=api-signup"])
    );
    assert_eq!(processes["api"]["working_dir"], "api");

    let graph = compose_graph(&manifest, &endpoints, &env, Path::new("/state/env"), false).unwrap();
    assert!(graph["processes"].get("setup").is_none());
    assert_eq!(
        graph["processes"]["prestart"]["depends_on"],
        json!({"db": {"condition": "process_healthy"}})
    );
}

#[test]
fn every_rule_has_one_error() {
    let minimal =
        "version: 1\nproject: app\nservices:\n  web: {command: bin/web, http: {healthcheck: /}}\n";
    Manifest::parse(minimal).unwrap();
    for (text, expected) in [
        (
            "project: app\nservices: {}\n".to_owned(),
            "add `version: 1` at the top of parcela.yaml",
        ),
        (
            minimal.replace("version: 1", "version: 2"),
            "unsupported version 2; this Parcela reads version 1",
        ),
        (
            minimal.replace("project: app\n", ""),
            "missing field `project`",
        ),
        (
            minimal.replace(
                "http: {healthcheck: /}",
                "http: {subdomain: www, healthcheck: /}",
            ),
            "one service needs http without a subdomain, like `http: {healthcheck: /}`; it gets the environment's URL",
        ),
        (
            format!("{minimal}  admin: {{command: bin/admin, http: {{healthcheck: /}}}}\n"),
            "only one http service can leave out subdomain; it gets the environment's own URL",
        ),
        (
            minimal.replace(
                "http: {healthcheck: /}",
                "http: {healthcheck: /}, healthcheck: 'true'",
            ),
            "service web has http, so its readiness is http.healthcheck; remove healthcheck",
        ),
        (
            minimal.replace("command: bin/web,", "command: bin/web, depends_on: [db],"),
            "service web depends on db, which is not a service",
        ),
        (
            format!(
                "{minimal}  jobs: {{command: bin/jobs, depends_on: [worker]}}\n  worker: {{command: bin/worker, depends_on: [jobs]}}\n"
            ),
            "depends_on forms a cycle: jobs -> worker -> jobs",
        ),
        (
            format!("{minimal}hooks:\n  setup: {{command: bin/setup, depends_on: [db]}}\n"),
            "hooks.setup depends on db, which is not a service",
        ),
        (
            format!("{minimal}  setup: {{command: bin/setup}}\n"),
            "setup is reserved for hooks.setup; rename the service",
        ),
        (
            format!("{minimal}env: {{PORT: '1'}}\n"),
            "invalid env: PORT is set by Parcela",
        ),
        (
            minimal.replace("healthcheck: /", "healthcheck: up"),
            "service web: http.healthcheck must be a path like /up",
        ),
    ] {
        let message = error(&text);
        assert!(message.contains(expected), "{text}\n{message}");
    }
    assert!(error(&format!("{minimal}workers: {{}}\n")).contains("unknown field `workers`"));
    assert!(
        error(&minimal.replace("command: bin/web,", "cmd: bin/web,"))
            .contains("unknown field `cmd`")
    );
}

#[test]
fn named_ports_reject_invalid_names() {
    let head =
        "version: 1\nproject: app\nservices: {web: {command: bin/dev, http: {healthcheck: /}}}\n";
    assert_eq!(
        Manifest::parse(&format!("{head}ports: [db, vite-hmr]\n"))
            .unwrap()
            .ports,
        ["db", "vite-hmr"]
    );
    assert_eq!(
        parcela::config::port_variable("vite-hmr"),
        "PARCELA_PORT_VITE_HMR"
    );
    for (ports, message) in [
        ("[Db]", "DNS label"),
        ("[db_1]", "DNS label"),
        ("[db, db]", "duplicate port name: db"),
        ("db", "invalid type"),
    ] {
        let message_text = error(&format!("{head}ports: {ports}\n"));
        assert!(message_text.contains(message), "{ports}: {message_text}");
    }
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
fn linked_checkouts_suggest_the_main_checkout_project_name() {
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
        let name = parcela::config::inferred_project_name(&t.path().join(directory)).unwrap();
        assert_eq!(name, expected, "{directory}");
    }
}
