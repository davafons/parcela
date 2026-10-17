use parcela::{
    config::Manifest,
    control::Server,
    registry::{Registry, Role},
    sockets,
};
use std::{
    fs,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    sync::Mutex,
};
use tempfile::TempDir;

static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn set_runtime_dir(value: Option<&Path>) {
    // SAFETY: every test in this binary holds ENVIRONMENT while it reads or writes the variable.
    unsafe {
        match value {
            Some(value) => std::env::set_var("XDG_RUNTIME_DIR", value),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }
}

fn long_state(temp: &TempDir) -> (Registry, PathBuf, String) {
    let state = temp.path().join("s".repeat(80)).join("t".repeat(80));
    assert!(state.as_os_str().len() > 150);
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let manifest: Manifest = serde_saphyr::from_str(
        "version: 1\nproject: {id: app, name: app}\ndefault_service: web\nservices: {web: {}}\n",
    )
    .unwrap();
    let mut registry = Registry::open(&state, true).unwrap().unwrap();
    let workspace = registry
        .adopt(&manifest, &source, Role::Preview, "auth")
        .unwrap();
    let runtime = registry.runtime_path(&workspace);
    (registry, runtime, workspace.id)
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().mode() & 0o777
}

fn hashed_directory(socket: &Path, base: &Path) -> String {
    let directory = socket.parent().unwrap();
    assert_eq!(directory.parent().unwrap(), base);
    directory.file_name().unwrap().to_str().unwrap().to_owned()
}

#[test]
fn long_state_directories_bind_short_private_sockets() {
    let _guard = ENVIRONMENT.lock().unwrap();
    let temp = TempDir::new_in("/tmp").unwrap();
    let (registry, runtime, id) = long_state(&temp);
    let prefix: String = id.replace('-', "")[..12].to_owned();
    assert!(runtime.join("pc.sock").as_os_str().len() > 150);

    let xdg = temp.path().join("xdg");
    fs::create_dir(&xdg).unwrap();
    fs::set_permissions(&xdg, fs::Permissions::from_mode(0o700)).unwrap();
    set_runtime_dir(Some(&xdg));
    let environment = sockets::environment(&runtime).unwrap();
    let admin = sockets::proxy_admin(registry.state_path()).unwrap();
    let name = hashed_directory(&environment.control, &xdg);
    assert!(name.starts_with("parcela-") && name.len() == "parcela-".len() + 8);
    assert_eq!(
        environment.control.file_name().unwrap().to_str().unwrap(),
        format!("{prefix}-ctl.sock")
    );
    assert_eq!(
        environment.compose,
        environment
            .control
            .with_file_name(format!("{prefix}-pc.sock"))
    );
    assert_eq!(
        admin,
        environment.control.with_file_name("proxy-admin.sock")
    );

    let server = Server::bind(&runtime).unwrap();
    assert!(environment.control.as_os_str().len() < 100);
    assert_eq!(mode(environment.control.parent().unwrap()), 0o700);
    assert_eq!(mode(&environment.control), 0o600);
    sockets::prepare(&admin).unwrap();
    let listener = UnixListener::bind(&admin).unwrap();
    drop((server, listener));
    assert!(!environment.control.exists());
    fs::remove_file(&admin).unwrap();

    set_runtime_dir(None);
    let fallback = sockets::environment(&runtime).unwrap();
    // SAFETY: getuid has no pointers or failure case.
    let base = PathBuf::from(format!("/tmp/parcela-{}", unsafe { libc::getuid() }));
    assert_eq!(hashed_directory(&fallback.compose, &base).len(), 8);
    assert!(fallback.compose.as_os_str().len() < 100);
    let server = Server::bind(&runtime).unwrap();
    assert_eq!(mode(&base), 0o700);
    assert_eq!(mode(fallback.control.parent().unwrap()), 0o700);
    drop(server);
    fs::remove_dir(fallback.control.parent().unwrap()).unwrap();
    assert_eq!(
        sockets::environment(&runtime).unwrap().control,
        fallback.control
    );
}

#[test]
fn shared_or_linked_socket_directories_are_refused() {
    let _guard = ENVIRONMENT.lock().unwrap();
    let temp = TempDir::new_in("/tmp").unwrap();
    let (registry, runtime, _) = long_state(&temp);
    let xdg = temp.path().join("xdg");
    fs::create_dir(&xdg).unwrap();
    fs::set_permissions(&xdg, fs::Permissions::from_mode(0o700)).unwrap();
    set_runtime_dir(Some(&xdg));
    let socket = sockets::environment(&runtime).unwrap().control;
    let directory = socket.parent().unwrap().to_owned();

    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
    let error = Server::bind(&runtime).err().unwrap();
    assert!(format!("{error:#}").contains("private directory (700)"));
    assert!(sockets::prepare(&sockets::proxy_admin(registry.state_path()).unwrap()).is_err());
    assert!(!socket.exists());
    assert_eq!(mode(&directory), 0o777);

    fs::remove_dir(&directory).unwrap();
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &directory).unwrap();
    assert!(Server::bind(&runtime).is_err());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());

    fs::remove_file(&directory).unwrap();
    fs::set_permissions(&xdg, fs::Permissions::from_mode(0o755)).unwrap();
    // SAFETY: getuid has no pointers or failure case.
    let base = PathBuf::from(format!("/tmp/parcela-{}", unsafe { libc::getuid() }));
    let fallback = sockets::environment(&runtime).unwrap().control;
    assert_eq!(fallback.parent().unwrap().parent().unwrap(), base);
    set_runtime_dir(None);
}

#[test]
fn over_limit_socket_paths_name_the_limit_and_path() {
    let _guard = ENVIRONMENT.lock().unwrap();
    let temp = TempDir::new_in("/tmp").unwrap();
    let (_registry, runtime, _) = long_state(&temp);
    let xdg = temp.path().join("x".repeat(90));
    fs::create_dir(&xdg).unwrap();
    fs::set_permissions(&xdg, fs::Permissions::from_mode(0o700)).unwrap();
    set_runtime_dir(Some(&xdg));
    let error = format!("{:#}", sockets::environment(&runtime).err().unwrap());
    set_runtime_dir(None);
    assert!(error.contains("the limit is 99 bytes"), "{error}");
    assert!(error.contains(&xdg.display().to_string()), "{error}");
}
