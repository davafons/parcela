use parcela::{
    config::Manifest,
    private,
    registry::{Registry, Role},
    router::{self, Router},
    runtime,
};
use std::{fs, os::unix::fs::symlink};
use tempfile::TempDir;
use uuid::Uuid;
fn fixture() -> (TempDir, Registry, Manifest, parcela::registry::Workspace) {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let manifest: Manifest = serde_saphyr::from_str("version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {readiness_path: /up}}\nenvironment: {DATA_DIR: '${PARCELA_RUNTIME_DIR}/data'}\n").unwrap();
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    let workspace = registry
        .adopt(&manifest, &source, Role::Preview, "auth")
        .unwrap();
    (temp, registry, manifest, workspace)
}
#[test]
fn lease_token_generation_and_interrupted_ownership() {
    let (_temp, mut registry, _manifest, workspace) = fixture();
    let token = Uuid::new_v4().to_string();
    let session = registry
        .begin_session(
            &workspace,
            &token,
            32001,
            "run",
            "https://auth-app.localhost",
        )
        .unwrap();
    assert_eq!(session.generation, 2);
    assert!(
        registry
            .begin_session(
                &workspace,
                &token,
                32002,
                "run",
                "https://auth-app.localhost"
            )
            .is_err()
    );
    let mut wrong = session.clone();
    wrong.token = Uuid::new_v4().to_string();
    assert!(registry.session_status(&wrong, "stopped").is_err());
    registry.session_status(&session, "ready").unwrap();
    registry.session_status(&session, "stopped").unwrap();
    let next = registry
        .begin_session(
            &workspace,
            &token,
            32002,
            "run",
            "https://auth-app.localhost",
        )
        .unwrap();
    assert_eq!(next.generation, 3);
    assert!(registry.session_status(&session, "ready").is_err());
}
#[test]
fn port_conflict_rolls_back_generation() {
    let (temp, mut registry, manifest, workspace) = fixture();
    let other = temp.path().join("other");
    fs::create_dir(&other).unwrap();
    let other = registry
        .adopt(&manifest, &other, Role::Preview, "other")
        .unwrap();
    let token = Uuid::new_v4().to_string();
    registry
        .begin_session(
            &workspace,
            &token,
            32001,
            "run",
            "https://auth-app.localhost",
        )
        .unwrap();
    assert!(
        registry
            .begin_session(&other, &token, 32001, "run", "https://other-app.localhost")
            .is_err()
    );
    assert_eq!(
        registry
            .current(&other.source, &manifest)
            .unwrap()
            .generation,
        1
    );
    assert!(registry.session(&other.id).unwrap().is_none());
}
#[test]
fn caddy_config_is_exact_owned_loopback_no_trust_no_persist() {
    let (_temp, mut registry, _manifest, workspace) = fixture();
    let router = Router {
        token: Uuid::new_v4().to_string(),
        https_port: 34443,
        http_port: 34080,
        test_profile: true,
        forwarded_standard_ports: false,
    };
    let session = registry
        .begin_session(
            &workspace,
            &router.token,
            32001,
            "run",
            &router.url(&workspace.hostname),
        )
        .unwrap();
    let before = router::configuration(&registry, &router).unwrap();
    assert_eq!(
        before["apps"]["http"]["servers"]["https"]["routes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    registry.session_status(&session, "ready").unwrap();
    let config = router::configuration(&registry, &router).unwrap();
    assert_eq!(config["admin"]["config"]["persist"], false);
    assert_eq!(
        config["apps"]["pki"]["certificate_authorities"]["local"]["install_trust"],
        false
    );
    assert_eq!(
        config["apps"]["http"]["servers"]["https"]["listen"][0],
        "127.0.0.1:34443"
    );
    assert_eq!(
        config["apps"]["http"]["servers"]["https"]["routes"][0]["match"][0]["host"][0],
        "auth-app.localhost"
    );
    assert_eq!(
        config["apps"]["tls"]["automation"]["policies"][0]["issuers"][0]["module"],
        "internal"
    );
    registry.session_status(&session, "stopping").unwrap();
    assert_eq!(router::configuration(&registry,&router).unwrap()["apps"]["http"]["servers"]["https"]["routes"].as_array().unwrap().len(),2);
    let replacement = Router {
        token: Uuid::new_v4().to_string(),
        ..router
    };
    registry.session_status(&session, "ready").unwrap();
    assert_eq!(
        router::configuration(&registry, &replacement).unwrap()["apps"]["http"]["servers"]["https"]
            ["routes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn context_placeholders_reserved_names_and_readiness_validation() {
    let (_temp, registry, mut manifest, workspace) = fixture();
    let env = runtime::environment(
        &manifest,
        &workspace,
        &registry.runtime_path(&workspace),
        "https://auth-app.localhost",
        Some(32001),
    )
    .unwrap();
    assert!(env["DATA_DIR"].ends_with("/data"));
    assert_eq!(env["PORT"], "32001");
    assert_eq!(env["HOST"], "127.0.0.1");
    assert_eq!(env["PARCELA_ROLE"], "preview");
    manifest.environment.insert("PORT".into(), "1234".into());
    assert!(manifest.validate().is_err());
    manifest.environment.remove("PORT");
    manifest
        .environment
        .insert("MISSING".into(), "${WRONG}".into());
    assert!(
        runtime::environment(
            &manifest,
            &workspace,
            &registry.runtime_path(&workspace),
            "https://auth-app.localhost",
            None
        )
        .is_err()
    );
    manifest.services.get_mut("web").unwrap().readiness_path = "/up\r\nBad: true".into();
    assert!(manifest.validate().is_err());
}
#[test]
fn kernel_locks_and_private_files_reject_symlinks() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("private");
    private::directory(&root).unwrap();
    let path = root.join("lock");
    let lock = private::Lock::acquire(&path).unwrap();
    assert!(private::Lock::acquire(&path).is_err());
    assert!(private::Lock::try_acquire(&path).unwrap().is_none());
    drop(lock);
    assert!(private::Lock::try_acquire(&path).unwrap().is_some());
    let link = root.join("link");
    symlink(&path, &link).unwrap();
    assert!(private::Lock::acquire(&link).is_err());
    assert!(private::Lock::try_acquire(&link).is_err());
    let metadata = root.join("metadata.json");
    private::write_json(&metadata, &serde_json::json!({"id":"owned"})).unwrap();
    let value: serde_json::Value = private::read_json(&metadata).unwrap();
    assert_eq!(value["id"], "owned");
    let linked = root.join("linked.json");
    symlink(&metadata, &linked).unwrap();
    assert!(private::read_json::<serde_json::Value>(&linked).is_err());
    assert!(private::write_json(&linked, &serde_json::json!({"id":"wrong"})).is_err());
}
#[test]
fn old_registry_migration_is_explicit_and_preserves_ids() {
    let (temp, registry, manifest, workspace) = fixture();
    drop(registry);
    let state = temp.path().join("state");
    let connection = rusqlite::Connection::open(state.join("registry.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TABLE sessions; PRAGMA user_version=1;")
        .unwrap();
    drop(connection);
    let old = Registry::open(&state, false).unwrap().unwrap();
    assert!(old.session(&workspace.id).unwrap().is_none());
    drop(old);
    let migrated = Registry::open(&state, true).unwrap().unwrap();
    assert_eq!(
        migrated.current(&workspace.source, &manifest).unwrap().id,
        workspace.id
    );
    assert!(migrated.sessions().unwrap().is_empty());
}

#[test]
fn explicit_forwarding_keeps_public_url_port_free_without_changing_listeners() {
    let (_temp, registry, _manifest, _workspace) = fixture();
    let router = Router {
        token: Uuid::new_v4().to_string(),
        https_port: 8443,
        http_port: 8080,
        test_profile: false,
        forwarded_standard_ports: true,
    };
    assert_eq!(
        router.url("auth-app.localhost"),
        "https://auth-app.localhost"
    );
    let config = router::configuration(&registry, &router).unwrap();
    assert_eq!(
        config["apps"]["http"]["servers"]["https"]["listen"][0],
        "127.0.0.1:8443"
    );
    assert_eq!(
        config["apps"]["http"]["servers"]["redirect"]["routes"][1]["handle"][0]["body"],
        format!("parcela-{}", router.token)
    );
}
