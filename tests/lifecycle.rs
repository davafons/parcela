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
    caller.shutdown(std::net::Shutdown::Both).unwrap();
    drop(caller);
    let pending = server.receive(&session).unwrap().unwrap();
    assert!(matches!(pending.action, Action::Stop));
    assert!(
        pending
            .reply(
                &session,
                Ok(serde_json::json!({"stop_accepted": true, "unread": "x".repeat(1_000_000)})),
            )
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
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let pending = loop {
            if let Some(pending) = server.receive(&session).unwrap() {
                break pending;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "malformed-response fixture received no request"
            );
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

#[test]
fn contending_stop_waits_for_same_generation_completion() {
    contending_stop(false);
}

#[test]
fn contending_stop_refuses_a_successor_generation() {
    contending_stop(true);
}

fn contending_stop(replace_session: bool) {
    use parcela::owner::{Phase, Record};

    let (_temp, mut registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    private::directory(&path).unwrap();
    let lock = private::Lock::acquire(&path.join("owner.lock")).unwrap();
    let control = Server::bind(&path).unwrap();
    let mut child = process::OwnedChild::spawn(Command::new("sleep").arg("60")).unwrap();
    let identity = process::Identity::read(child.id()).unwrap().unwrap();
    child.stop(libc::SIGTERM).unwrap();
    let mut record = Record {
        session: session.clone(),
        phase: Phase::Stopping,
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
    registry.session_status(&session, "stopping").unwrap();
    let state = registry.state_path().to_path_buf();
    let expected = session.clone();
    let target = workspace.clone();
    let owner = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        let mut writer = Registry::open(&state, true).unwrap().unwrap();
        record.phase = Phase::Stopped;
        record.save(&path).unwrap();
        writer.session_status(&expected, "stopped").unwrap();
        let successor = replace_session.then(|| {
            writer
                .begin_session(
                    &target,
                    &expected.router_token,
                    32002,
                    "compose",
                    &expected.public_url,
                )
                .unwrap()
        });
        drop(lock);
        successor
    });
    let result = lifecycle::down(&registry, &workspace);
    let successor = owner.join().unwrap();
    let current = registry.session(&workspace.id).unwrap().unwrap();
    if let Some(successor) = successor {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("generation changed")
        );
        assert_eq!(current.token, successor.token);
        assert_eq!(current.status, "starting");
    } else {
        assert_eq!(result.unwrap()["state"], "stopped");
        assert_eq!(current.token, session.token);
    }
}

#[test]
fn waiting_stop_reaches_the_recovered_same_generation_controller() {
    waiting_stop(false);
}

#[test]
fn waiting_stop_never_reaches_a_replaced_control_socket() {
    waiting_stop(true);
}

fn waiting_stop(replace_control: bool) {
    use parcela::owner::{Phase, Record};
    use std::time::Instant;

    let (_temp, mut registry, workspace, session) = fixture();
    let path = registry.runtime_path(&workspace);
    private::directory(&path).unwrap();
    let lock = private::Lock::acquire(&path.join("owner.lock")).unwrap();
    let control = Server::bind(&path).unwrap();
    let mut child = process::OwnedChild::spawn(Command::new("sleep").arg("60")).unwrap();
    let identity = process::Identity::read(child.id()).unwrap().unwrap();
    child.stop(libc::SIGTERM).unwrap();
    let mut record = Record {
        session: session.clone(),
        phase: Phase::Starting,
        owner: process::Identity::read(std::process::id()).unwrap().unwrap(),
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
    let (control, retained_control) = if replace_control {
        fs::rename(path.join("ctl.sock"), path.join("retained.sock")).unwrap();
        (Server::bind(&path).unwrap(), Some(control))
    } else {
        (control, None)
    };
    let retained_path = path.join("retained.sock");
    let state = registry.state_path().to_path_buf();
    let expected = session.clone();
    let owner = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let admitted = loop {
            if let Some(pending) = control.receive(&expected).unwrap() {
                assert!(matches!(pending.action, Action::Stop));
                record.phase = Phase::Stopping;
                record.save(&path).unwrap();
                let mut writer = Registry::open(&state, true).unwrap().unwrap();
                writer.session_status(&expected, "stopping").unwrap();
                pending
                    .reply(&expected, Ok(serde_json::json!({"stop_accepted":true})))
                    .unwrap();
                record.phase = Phase::Stopped;
                record.save(&path).unwrap();
                writer.session_status(&expected, "stopped").unwrap();
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            thread::sleep(Duration::from_millis(5));
        };
        drop(control);
        drop(lock);
        admitted
    });
    let result = parcela::runtime::recover(&mut registry, &workspace, &session, true);
    let admitted = owner.join().unwrap();
    if replace_control {
        assert!(!admitted, "stop reached a replaced control socket");
        assert!(result.is_err());
        assert_eq!(
            registry.session(&workspace.id).unwrap().unwrap().status,
            "starting"
        );
        assert!(retained_path.exists());
    } else {
        assert!(admitted, "stop never reached the recovered controller");
        assert_eq!(result.unwrap()["state"], "stopped");
    }
    drop(retained_control);
    assert_eq!(
        registry.session(&workspace.id).unwrap().unwrap().token,
        session.token
    );
}
