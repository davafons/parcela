use parcela::{
    config::{Manifest, dns_label, locate},
    context::ResolvedContext,
    registry::{Registry, Role},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::{Command, Output},
    sync::{Arc, Barrier},
    thread,
};
use tempfile::TempDir;

const MANIFEST: &str = "version: 1\nproject: {id: demo-id, name: demo}\ndefault_service: web\nservices: {web: {}, api: {}}\ntasks: {test: {argv: [python3, -m, unittest], timeout_seconds: 60}}\nprocess_compose: {version: '0.5', processes: {web: {command: echo hi, custom_native_key: kept}, api: {command: echo api}}}\n";
fn manifest() -> Manifest {
    serde_saphyr::from_str(MANIFEST).unwrap()
}
fn checkout(root: &Path, name: &str) -> std::path::PathBuf {
    let path = root.join(name);
    fs::create_dir(&path).unwrap();
    fs::write(path.join("parcela.yaml"), MANIFEST).unwrap();
    path.canonicalize().unwrap()
}
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
fn value(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn strict_schema_and_native_graph_preservation() {
    let manifest = manifest();
    manifest.validate().unwrap();
    assert_eq!(
        manifest.process_compose.unwrap()["processes"]["web"]["custom_native_key"],
        "kept"
    );
    assert!(serde_saphyr::from_str::<Manifest>(&format!("{MANIFEST}typo: true\n")).is_err());
    assert!(
        serde_saphyr::from_str::<Manifest>(&MANIFEST.replace("web: {}", "web: {typo: true}"))
            .is_err()
    );
    assert!(
        serde_saphyr::from_str::<Manifest>(&MANIFEST.replace("timeout_seconds: 60", "typo: 60"))
            .is_err()
    );
    assert!(serde_saphyr::from_str::<Manifest>(&format!("{MANIFEST}version: 2\n")).is_err());
}
#[test]
fn validation_rejects_bad_contracts() {
    for text in [
        MANIFEST.replace("version: 1", "version: 2"),
        MANIFEST.replace("default_service: web", "default_service: absent"),
        MANIFEST.replace("name: demo", "name: ../escape"),
        MANIFEST.replace("argv: [python3, -m, unittest]", "argv: []"),
        MANIFEST.replace("timeout_seconds: 60", "timeout_seconds: 0"),
        MANIFEST.replace("web: {}", "web: {subdomain: www}"),
        MANIFEST.replace("api: {}", "api: {}, jobs: {subdomain: api}"),
    ] {
        assert!(
            serde_saphyr::from_str::<Manifest>(&text)
                .unwrap()
                .validate()
                .is_err(),
            "{text}"
        );
    }
    for label in ["", "Upper", "a.b", "-a", "a-", "../a", "á", &"a".repeat(64)] {
        assert!(dns_label(label).is_err());
    }
    dns_label("auth-2").unwrap();
    assert!(manifest().hostname(false, &"a".repeat(60)).is_err());
}
#[test]
fn flat_planned_urls_and_explicit_no_runtime_claims() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    let m = manifest();
    let primary = registry.adopt(&m, &path, Role::Primary, "main").unwrap();
    assert_eq!(primary.hostname, "demo.localhost");
    let preview_path = checkout(temp.path(), "preview");
    let preview = registry
        .adopt(&m, &preview_path, Role::Preview, "auth")
        .unwrap();
    let context = ResolvedContext::new(&registry, &m, preview).unwrap();
    assert_eq!(context.planned_urls["web"], "https://auth-demo.localhost");
    assert_eq!(
        context.planned_urls["api"],
        "https://api-auth-demo.localhost"
    );
    assert!(!context.capabilities.routing && !context.capabilities.data_isolation_verified);
    assert!(!context.runtime_dir.exists());
    assert!(context.source_exists);
}
#[test]
fn identities_survive_reopen_and_metadata_changes() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    fs::create_dir(path.join(".jj")).unwrap();
    fs::create_dir(path.join(".git")).unwrap();
    let state = temp.path().join("state");
    let m = manifest();
    let id = {
        let mut registry = Registry::open(&state, true).unwrap().unwrap();
        let workspace = registry.adopt(&m, &path, Role::Preview, "auth").unwrap();
        assert_eq!(workspace.vcs, "jj");
        workspace.id
    };
    fs::write(path.join(".git/HEAD"), "detached-revision").unwrap();
    let mut registry = Registry::open(&state, true).unwrap().unwrap();
    assert_eq!(
        registry.adopt(&m, &path, Role::Preview, "auth").unwrap().id,
        id
    );
    assert!(registry.adopt(&m, &path, Role::Primary, "auth").is_err());
    assert!(registry.adopt(&m, &path, Role::Preview, "new").is_err());
    let mut changed = manifest();
    changed.project.name = "changed".into();
    assert!(registry.current(&path, &changed).is_err());
    assert_eq!(registry.current(&path, &m).unwrap().id, id);
}
#[test]
fn conflicts_do_not_steal_ownership_or_leave_projects() {
    let temp = TempDir::new().unwrap();
    let a = checkout(temp.path(), "a");
    let b = checkout(temp.path(), "b");
    let state = temp.path().join("state");
    let mut registry = Registry::open(&state, true).unwrap().unwrap();
    let m = manifest();
    registry.adopt(&m, &a, Role::Preview, "auth").unwrap();
    assert!(registry.adopt(&m, &b, Role::Preview, "auth").is_err());
    let mut other = manifest();
    other.project.id = "different".into();
    other.project.name = "auth-demo".into();
    assert!(registry.adopt(&other, &b, Role::Primary, "main").is_err());
    let connection = rusqlite::Connection::open(state.join("registry.sqlite3")).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(registry.list(None).unwrap().len(), 1);
}
#[test]
fn only_one_primary_per_project_and_explicit_scoping() {
    let temp = TempDir::new().unwrap();
    let a = checkout(temp.path(), "a");
    let b = checkout(temp.path(), "b");
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    let m = manifest();
    registry.adopt(&m, &a, Role::Primary, "main").unwrap();
    assert!(registry.adopt(&m, &b, Role::Primary, "other").is_err());
    let mut other = manifest();
    other.project.id = "different".into();
    other.project.name = "other".into();
    registry.adopt(&other, &b, Role::Primary, "main").unwrap();
    assert_eq!(registry.list(Some("demo-id")).unwrap().len(), 1);
    assert_eq!(registry.list(None).unwrap().len(), 2);
}
#[test]
fn symlinked_source_is_same_identity_and_nested_path_finds_checkout() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    fs::create_dir(path.join("src")).unwrap();
    assert_eq!(locate(&path.join("src")).unwrap(), path);
    let link = temp.path().join("link");
    symlink(&path, &link).unwrap();
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    let a = registry
        .adopt(&manifest(), &path, Role::Preview, "auth")
        .unwrap();
    assert_eq!(
        registry
            .adopt(&manifest(), &link, Role::Preview, "auth")
            .unwrap()
            .id,
        a.id
    );
}
#[test]
fn inspection_does_not_create_state_and_cli_requires_explicit_role() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let state = temp.path().join("state");
    value(&cli(&path, &state, &["config", "check"]));
    assert_eq!(
        value(&cli(&path, &state, &["workspace", "list"]))["workspaces"],
        serde_json::json!([])
    );
    value(&cli(temp.path(), &state, &["workspace", "list", "--all"]));
    assert!(!cli(&path, &state, &["context"]).status.success());
    assert!(
        !cli(&path, &state, &["workspace", "adopt", "--label", "auth"])
            .status
            .success()
    );
    assert!(
        !cli(
            &path,
            &state,
            &[
                "workspace",
                "adopt",
                "--role",
                "preview",
                "--label",
                "../bad"
            ]
        )
        .status
        .success()
    );
    assert!(!state.exists());
}
#[test]
fn cli_json_errors_and_source_preservation() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let state = temp.path().join("state");
    let before = fs::read(path.join("parcela.yaml")).unwrap();
    let adopted = value(&cli(
        &path,
        &state,
        &["workspace", "adopt", "--role", "preview", "--label", "auth"],
    ));
    let context = value(&cli(&path, &state, &["context"]));
    assert_eq!(adopted, context);
    assert_eq!(before, fs::read(path.join("parcela.yaml")).unwrap());
    assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
    let unsupported = cli(&path, &state, &["up"]);
    assert!(!unsupported.status.success() && unsupported.stdout.is_empty());
    let error = cli(
        &path,
        &state,
        &["workspace", "adopt", "--role", "primary", "--label", "main"],
    );
    assert!(!error.status.success() && error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&error.stderr).contains("implicit rebind"));
}
#[test]
fn private_state_and_database_permissions_and_symlinks() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("state");
    drop(Registry::open(&state, true).unwrap());
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(state.join("registry.sqlite3"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let link = temp.path().join("link");
    symlink(&state, &link).unwrap();
    assert!(Registry::open(&link, false).is_err());
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Registry::open(&state, true).is_err());
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let foreign = temp.path().join("foreign");
    fs::write(&foreign, "unrelated").unwrap();
    fs::remove_file(state.join("registry.sqlite3")).unwrap();
    symlink(&foreign, state.join("registry.sqlite3")).unwrap();
    assert!(Registry::open(&state, true).is_err());
    assert_eq!(fs::read_to_string(&foreign).unwrap(), "unrelated");
}
#[test]
fn malformed_and_future_state_fail_closed() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("state");
    drop(Registry::open(&state, true).unwrap());
    let db = state.join("registry.sqlite3");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.pragma_update(None, "user_version", 999).unwrap();
    assert!(Registry::open(&state, true).is_err());
    assert!(Registry::open(&state, false).is_err());
    drop(connection);
    fs::write(&db, "not a sqlite database").unwrap();
    assert!(Registry::open(&state, true).is_err());
    assert_eq!(fs::read_to_string(db).unwrap(), "not a sqlite database");
}
#[test]
fn corrupted_identity_cannot_become_runtime_path() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("state");
    let path = checkout(temp.path(), "app");
    let mut registry = Registry::open(&state, true).unwrap().unwrap();
    registry
        .adopt(&manifest(), &path, Role::Preview, "auth")
        .unwrap();
    let connection = rusqlite::Connection::open(state.join("registry.sqlite3")).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .unwrap();
    connection
        .execute("UPDATE workspaces SET id = '../../foreign'", [])
        .unwrap();
    assert!(registry.list(None).is_err());
    assert!(registry.current(&path, &manifest()).is_err());
}
#[test]
fn missing_source_is_retained_not_deleted() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    registry
        .adopt(&manifest(), &path, Role::Preview, "auth")
        .unwrap();
    fs::remove_dir_all(path).unwrap();
    assert_eq!(registry.list(None).unwrap().len(), 1);
    let output = value(&cli(
        temp.path(),
        &temp.path().join("state"),
        &["workspace", "list", "--all"],
    ));
    assert_eq!(output["workspaces"][0]["source_exists"], false);
}
#[test]
fn concurrent_cli_adoption_is_idempotent_with_one_owner() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let state = temp.path().join("state");
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            let state = state.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                value(&cli(
                    &path,
                    &state,
                    &["workspace", "adopt", "--role", "preview", "--label", "auth"],
                ))["workspace"]["id"]
                    .clone()
            })
        })
        .collect();
    let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(ids.iter().all(|id| id == &ids[0]));
    let registry = Registry::open(&state, false).unwrap().unwrap();
    assert_eq!(registry.list(None).unwrap().len(), 1);
}
#[test]
fn concurrent_conflicting_adoption_has_exactly_one_winner() {
    let temp = TempDir::new().unwrap();
    let a = checkout(temp.path(), "a");
    let b = checkout(temp.path(), "b");
    let state = temp.path().join("state");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [a, b]
        .into_iter()
        .map(|path| {
            let state = state.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                cli(
                    &path,
                    &state,
                    &["workspace", "adopt", "--role", "primary", "--label", "main"],
                )
                .status
                .success()
            })
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let registry = Registry::open(&state, false).unwrap().unwrap();
    assert_eq!(registry.list(None).unwrap().len(), 1);
}

#[test]
fn dangling_state_and_database_symlinks_fail_closed() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("state");
    symlink(temp.path().join("absent"), &state).unwrap();
    assert!(Registry::open(&state, false).is_err());
    assert!(Registry::open(&state, true).is_err());
    fs::remove_file(&state).unwrap();
    drop(Registry::open(&state, true).unwrap());
    fs::remove_file(state.join("registry.sqlite3")).unwrap();
    symlink(
        temp.path().join("absent-db"),
        state.join("registry.sqlite3"),
    )
    .unwrap();
    assert!(Registry::open(&state, false).is_err());
    assert!(Registry::open(&state, true).is_err());
}

#[test]
fn foreign_database_is_not_initialized_or_modified() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let db = state.join("registry.sqlite3");
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE unrelated (value TEXT); INSERT INTO unrelated VALUES ('keep');",
        )
        .unwrap();
    drop(connection);
    fs::set_permissions(&db, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(&db).unwrap();
    assert!(Registry::open(&state, true).is_err());
    assert_eq!(fs::read(&db).unwrap(), before);
}

#[test]
fn uuid_target_inspection_and_stop_survive_missing_source() {
    let temp = TempDir::new().unwrap();
    let path = checkout(temp.path(), "app");
    let state = temp.path().join("state");
    let adopted = value(&cli(
        &path,
        &state,
        &["workspace", "adopt", "--role", "preview", "--label", "auth"],
    ));
    let id = adopted["workspace"]["id"].as_str().unwrap();
    fs::remove_file(path.join("parcela.yaml")).unwrap();
    fs::remove_dir(&path).unwrap();
    for command in ["status", "down", "url"] {
        let result = Command::new(env!("CARGO_BIN_EXE_parcela"))
            .arg("--state-dir")
            .arg(&state)
            .args(["--workspace", id, command])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if command == "status" {
            assert_eq!(value(&result)["source_exists"], false);
            assert_eq!(value(&result)["state"], "stopped");
        }
    }
    assert!(!state.join("environments").exists());
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_parcela"))
            .arg("--state-dir")
            .arg(&state)
            .args([
                "--workspace",
                id,
                "--path",
                temp.path().to_str().unwrap(),
                "status"
            ])
            .output()
            .unwrap()
            .status
            .success()
    );
}
