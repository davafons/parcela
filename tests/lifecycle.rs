use parcela::{
    config::Manifest,
    control::{self, Action, Server},
    lifecycle, private, process,
    registry::{Registry, Role},
};
use std::{
    fs,
    os::unix::{fs::symlink, net::UnixListener},
    process::Command,
    thread,
    time::Duration,
};
use tempfile::TempDir;
use uuid::Uuid;

fn fixture() -> (
    TempDir,
    Registry,
    parcela::registry::Workspace,
    parcela::registry::Session,
) {
    let temp = TempDir::new_in("/tmp").unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let manifest: Manifest = serde_saphyr::from_str(
        "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}}\n",
    )
    .unwrap();
    let mut registry = Registry::open(&temp.path().join("state"), true)
        .unwrap()
        .unwrap();
    let workspace = registry
        .adopt(&manifest, &source, Role::Preview, "auth")
        .unwrap();
    let session = registry
        .begin_session(
            &workspace,
            &Uuid::new_v4().to_string(),
            32001,
            "compose",
            "https://auth-app.localhost",
        )
        .unwrap();
    (temp, registry, workspace, session)
}

#[test]
fn unavailable_owner_is_unknown_and_inspection_creates_no_runtime() {
    let (_temp, registry, workspace, _session) = fixture();
    let status = lifecycle::status(&registry, &workspace).unwrap();
    assert_eq!(status["state"], "unknown");
    assert_eq!(status["live_health_verified"], false);
    assert!(lifecycle::down(&registry, &workspace).is_err());
    assert!(!registry.runtime_path(&workspace).exists());
}

#[test]
fn repeated_down_is_noop_without_runtime_or_process_signaling() {
    let (_temp, mut registry, workspace, session) = fixture();
    registry.session_status(&session, "stopped").unwrap();
    for _ in 0..2 {
        assert_eq!(
            lifecycle::down(&registry, &workspace).unwrap()["already_stopped"],
            true
        );
    }
    assert!(!registry.runtime_path(&workspace).exists());
}

#[test]
fn stale_nonce_or_generation_never_reaches_owner_actions() {
    let (_temp, registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    let server = Server::bind(&path).unwrap();
    for nonce in [true, false] {
        let mut stale = session.clone();
        if nonce {
            stale.token = Uuid::new_v4().to_string();
        } else {
            stale.generation += 1;
        }
        use std::io::{Read, Write};
        let mut stream = std::os::unix::net::UnixStream::connect(path.join("ctl.sock")).unwrap();
        let request = serde_json::json!({"environment_id": stale.environment_id, "token": stale.token, "generation": stale.generation, "action": {"type": "stop"}});
        writeln!(stream, "{request}").unwrap();
        assert!(server.receive(&session).unwrap().is_none());
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("stale or foreign owner request"));
    }
    let root = path.clone();
    let selected = session.clone();
    let client = thread::spawn(move || control::request(&root, &selected, Action::Status));
    let pending = loop {
        if let Some(pending) = server.receive(&session).unwrap() {
            break pending;
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(matches!(pending.action, Action::Status));
    pending
        .reply(&session, Ok(serde_json::json!({"verified": true})))
        .unwrap();
    assert_eq!(client.join().unwrap().unwrap()["verified"], true);
    drop(server);
    assert!(!path.join("ctl.sock").exists());
}

#[test]
fn disconnected_caller_retains_complete_stop_and_cannot_fail_owner() {
    use std::{io::Write, os::unix::net::UnixStream};

    let (_temp, registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    let server = Server::bind(&path).unwrap();
    let request = serde_json::json!({"environment_id": session.environment_id, "token": session.token, "generation": session.generation, "action": {"type": "stop"}});
    let mut caller = UnixStream::connect(path.join("ctl.sock")).unwrap();
    writeln!(caller, "{request}").unwrap();
    drop(caller);
    let pending = server.receive(&session).unwrap().unwrap();
    assert!(matches!(pending.action, Action::Stop));
    assert!(
        pending
            .reply(&session, Ok(serde_json::json!({"stop_accepted": true})))
            .is_err()
    );
    assert!(server.receive(&session).unwrap().is_none());

    let mut caller = UnixStream::connect(path.join("ctl.sock")).unwrap();
    caller.write_all(b"{\"action\":").unwrap();
    drop(caller);
    assert!(server.receive(&session).unwrap().is_none());
}

#[test]
fn control_server_never_replaces_or_removes_foreign_sockets_or_links() {
    let (_temp, registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    private::directory(&path).unwrap();
    let foreign = UnixListener::bind(path.join("ctl.sock")).unwrap();
    assert!(Server::bind(&path).is_err());
    assert!(path.join("ctl.sock").exists());
    drop(foreign);
    fs::remove_file(path.join("ctl.sock")).unwrap();
    let owner = Server::bind(&path).unwrap();
    fs::rename(path.join("ctl.sock"), path.join("original.sock")).unwrap();
    let replacement = UnixListener::bind(path.join("ctl.sock")).unwrap();
    drop(owner);
    assert!(path.join("ctl.sock").exists());
    drop(replacement);
    fs::remove_file(path.join("ctl.sock")).unwrap();
    symlink(path.join("original.sock"), path.join("ctl.sock")).unwrap();
    assert!(control::request(&path, &session, Action::Stop).is_err());
    assert!(path.join("ctl.sock").is_symlink());
}

#[test]
fn backend_output_is_bounded_and_control_commands_have_deadlines() {
    let output = process::capture(
        Command::new("sh").args(["-c", "printf output; printf error >&2"]),
        Duration::from_secs(1),
        1024,
    )
    .unwrap();
    assert_eq!(output.stdout, b"output");
    assert_eq!(output.stderr, b"error");
    assert!(
        process::capture(
            Command::new("sh").args(["-c", "printf oversized"]),
            Duration::from_secs(1),
            2
        )
        .is_err()
    );
    assert!(
        process::capture(
            Command::new("sh").args(["-c", "sleep 10"]),
            Duration::from_millis(30),
            1024
        )
        .is_err()
    );
}

#[test]
fn malformed_owner_health_response_stays_unknown() {
    let (_temp, registry, workspace, session) = fixture();
    let server = Server::bind(&registry.runtime_path(&workspace)).unwrap();
    let owner = thread::spawn(move || {
        let pending = loop {
            if let Some(pending) = server.receive(&session).unwrap() {
                break pending;
            }
            thread::sleep(Duration::from_millis(10));
        };
        pending
            .reply(&session, Ok(serde_json::json!({"state": "ready"})))
            .unwrap();
    });
    let status = lifecycle::status(&registry, &workspace).unwrap();
    owner.join().unwrap();
    assert_eq!(status["state"], "unknown");
    assert_eq!(status["live_health_verified"], false);
}

#[test]
fn quiescent_interrupted_stop_converges_without_native_reauthentication() {
    use parcela::owner::{Phase, Record};

    let (_temp, mut registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    private::directory(&path).unwrap();
    let lock = private::Lock::acquire(&path.join("owner.lock")).unwrap();
    let control = Server::bind(&path).unwrap();
    let mut child = Command::new("sleep").arg("60").spawn().unwrap();
    let identity = process::Identity::read(child.id()).unwrap().unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let mut record = Record {
        session: session.clone(),
        phase: Phase::Running,
        owner: identity.clone(),
        lock: lock.identity().unwrap(),
        control: private::Identity::read(&path.join("ctl.sock")).unwrap(),
        supervisor: identity.clone(),
        processes: vec![identity],
        native: None,
        manifest: serde_saphyr::from_str(
            "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}}\n",
        )
        .unwrap(),
        diagnostic: None,
    };
    record.save(&path).unwrap();
    drop(control);
    drop(lock);
    assert!(lifecycle::down(&registry, &workspace).is_err());
    assert_ne!(
        registry.session(&workspace.id).unwrap().unwrap().status,
        "stopped"
    );

    let mut survivor = process::OwnedChild::spawn(Command::new("sleep").arg("60")).unwrap();
    let survivor_identity = process::Identity::read(survivor.id()).unwrap().unwrap();
    record.processes.push(survivor_identity.clone());
    record.phase = Phase::Stopping;
    record.save(&path).unwrap();
    registry.session_status(&session, "stopping").unwrap();
    assert!(lifecycle::down(&registry, &workspace).is_err());
    assert!(survivor_identity.alive());
    assert_eq!(
        registry.session(&workspace.id).unwrap().unwrap().status,
        "stopping"
    );
    survivor.stop(libc::SIGTERM).unwrap();
    assert_eq!(
        lifecycle::down(&registry, &workspace).unwrap()["state"],
        "stopped"
    );
    assert_eq!(
        registry.session(&workspace.id).unwrap().unwrap().status,
        "stopped"
    );
    assert_eq!(Record::read(&path, &session).unwrap().phase, Phase::Stopped);
    assert!(!path.join("ctl.sock").exists());
    assert_eq!(
        lifecycle::down(&registry, &workspace).unwrap()["already_stopped"],
        true
    );
}
