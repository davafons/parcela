use parcela::{checkpoint, config::Manifest};
use rusqlite::Connection;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
};
use tempfile::TempDir;

const BASE: &str = "version: 1\nproject: {id: checkpoint-demo, name: demo}\ndefault_service: web\nservices: {web: {}}\n";
const POLICY: &str = "transfer:\n  data:\n    database:\n      location: source\n      path: storage/app.db\n      snapshot: {method: sqlite-backup}\n    uploads:\n      location: runtime\n      path: uploads\n      snapshot: {method: quiescent-files}\n";

fn jj(source: &Path, arguments: &[&str]) -> String {
    let result = Command::new("jj")
        .args(["--config", "signing.behavior=\"drop\""])
        .current_dir(source)
        .arg("-R")
        .arg(source)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}

fn fixture(temp: &TempDir, policy: &str) -> (std::path::PathBuf, Manifest) {
    let source = temp.path().join("source");
    let result = Command::new("jj")
        .args(["--config", "signing.behavior=\"drop\""])
        .args(["git", "init", "--colocate"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::write(source.join(".gitignore"), "storage/\n").unwrap();
    fs::write(source.join("parcela.yaml"), format!("{BASE}{policy}")).unwrap();
    fs::write(
        source.join("current.txt"),
        "working-copy content, not a named branch\n",
    )
    .unwrap();
    let manifest = Manifest::load(&source.join("parcela.yaml")).unwrap();
    (source, manifest)
}

#[test]
fn pinned_jj_and_live_wal_database_round_trip_without_source_pointers() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, POLICY);
    fs::create_dir(source.join("storage")).unwrap();
    let database = Connection::open(source.join("storage/app.db")).unwrap();
    database.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE items (value TEXT); INSERT INTO items VALUES ('in the WAL');").unwrap();
    let runtime = temp.path().join("runtime");
    fs::create_dir_all(runtime.join("uploads/empty")).unwrap();
    fs::write(runtime.join("uploads/file"), "persistent upload").unwrap();
    fs::write(source.join("run-me"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(source.join("run-me"), fs::Permissions::from_mode(0o755)).unwrap();
    let source_bookmarks = jj(&source, &["bookmark", "list"]);
    let bundle = temp.path().join("checkpoint");
    let checkpoint = checkpoint::create(
        &source,
        &manifest,
        &bundle,
        Some(&runtime),
        true,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    assert_eq!(jj(&source, &["bookmark", "list"]), source_bookmarks);
    assert!(!checkpoint.agents_captured);
    assert_eq!(checkpoint.activation, "paused; no ownership transfer");
    fs::write(
        source.join("key.yaml"),
        "public API schema, not a private key",
    )
    .unwrap();
    fs::write(source.join("current.txt"), "edited after checkpoint").unwrap();
    database
        .execute("INSERT INTO items VALUES ('later')", [])
        .unwrap();
    let result = checkpoint::restore(&bundle, &temp.path().join("restored")).unwrap();
    assert_eq!(
        fs::read_to_string(result.source.join("current.txt")).unwrap(),
        "working-copy content, not a named branch\n"
    );
    assert_eq!(
        fs::read_to_string(result.runtime.join("uploads/file")).unwrap(),
        "persistent upload"
    );
    assert!(result.runtime.join("uploads/empty").is_dir());
    let restored = Connection::open(result.source.join("storage/app.db")).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM items", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        restored
            .query_row("SELECT value FROM items", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "in the WAL"
    );
    assert_eq!(
        jj(
            &result.source,
            &["log", "-r", "@-", "--no-graph", "-T", "commit_id"]
        ),
        checkpoint.revision
    );
    assert!(
        fs::metadata(result.source.join("run-me"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111
            != 0
    );
    assert!(result.source.join(".jj/repo").is_dir());
    assert!(result.source.join(".git").is_dir());
    assert!(!result.runtime.join("owner.lock").exists());
}

#[test]
fn linked_jj_workspace_restores_as_an_independent_repository() {
    let temp = TempDir::new().unwrap();
    let (source, _) = fixture(&temp, "transfer: {}\n");
    jj(&source, &["log", "-r", "@", "--no-graph"]);
    let linked = temp.path().join("linked");
    jj(
        &source,
        &["workspace", "add", "-r", "@", linked.to_str().unwrap()],
    );
    fs::write(linked.join("current.txt"), "linked workspace edit").unwrap();
    let manifest = Manifest::load(&linked.join("parcela.yaml")).unwrap();
    let bundle = temp.path().join("checkpoint");
    checkpoint::create(
        &linked,
        &manifest,
        &bundle,
        None,
        false,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    let result = checkpoint::restore(&bundle, &temp.path().join("restored")).unwrap();
    fs::rename(source, temp.path().join("source-moved-away")).unwrap();
    fs::rename(linked, temp.path().join("linked-moved-away")).unwrap();
    assert_eq!(
        fs::read_to_string(result.source.join("current.txt")).unwrap(),
        "linked workspace edit"
    );
    assert!(
        !jj(
            &result.source,
            &["log", "-r", "@-", "--no-graph", "-T", "commit_id"]
        )
        .is_empty()
    );
}

#[test]
fn corruption_undeclared_payloads_and_permissions_fail_before_restore() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, "transfer: {}\n");
    let bundle = temp.path().join("checkpoint");
    checkpoint::create(
        &source,
        &manifest,
        &bundle,
        None,
        false,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    let original = fs::read(bundle.join("source.bundle")).unwrap();
    fs::write(bundle.join("source.bundle"), "corrupt").unwrap();
    let destination = temp.path().join("restored");
    assert!(checkpoint::restore(&bundle, &destination).is_err());
    assert!(!destination.exists());
    fs::write(bundle.join("source.bundle"), original).unwrap();
    fs::write(bundle.join("extra"), "undeclared").unwrap();
    assert!(checkpoint::verify(&bundle).is_err());
    fs::remove_file(bundle.join("extra")).unwrap();
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(checkpoint::verify(&bundle).is_err());
}

#[test]
fn inspection_of_absent_checkpoint_does_not_create_state() {
    let temp = TempDir::new().unwrap();
    let absent = temp.path().join("absent");
    assert!(checkpoint::verify(&absent).is_err());
    assert!(!absent.exists());
}

#[test]
fn source_and_existing_destinations_are_never_overwritten() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, "transfer: {}\n");
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &source.join("checkpoint"),
            None,
            false,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert!(!source.join("checkpoint").exists());
    let alias = temp.path().join("alias");
    symlink(&source, &alias).unwrap();
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &alias.join("checkpoint"),
            None,
            false,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    let bundle = temp.path().join("checkpoint");
    checkpoint::create(
        &source,
        &manifest,
        &bundle,
        None,
        false,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    let manifest_bytes = fs::read(bundle.join("checkpoint.json")).unwrap();
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &bundle,
            None,
            false,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert_eq!(
        fs::read(bundle.join("checkpoint.json")).unwrap(),
        manifest_bytes
    );
    assert!(checkpoint::restore(&bundle, &source).is_err());
    assert!(source.join("current.txt").exists());
}

#[test]
fn unsupported_profiles_tasks_and_unquiesced_files_are_not_omitted() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, POLICY);
    let output = temp.path().join("checkpoint");
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &output,
            None,
            false,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert!(!output.exists());
    let mut config = serde_json::to_value(manifest).unwrap();
    config["transfer"]["profiles"] = serde_json::json!({"yuke": {"include": ["index.js"]}});
    let manifest: Manifest = serde_json::from_value(config).unwrap();
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &output,
            None,
            true,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert!(!output.exists());
}

#[test]
fn resource_symlinks_credentials_and_traversal_are_rejected() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer: {data: {files: {location: source, path: storage, snapshot: {method: quiescent-files}}}}\n";
    let (source, manifest) = fixture(&temp, policy);
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("file"), "outside").unwrap();
    symlink(&outside, source.join("storage")).unwrap();
    let output = temp.path().join("checkpoint");
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &output,
            None,
            true,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert!(!output.exists());
    fs::remove_file(source.join("storage")).unwrap();
    fs::create_dir(source.join("storage")).unwrap();
    fs::write(
        source.join("storage/auth.json"),
        "never transfer this credential",
    )
    .unwrap();
    assert!(
        checkpoint::create(
            &source,
            &manifest,
            &output,
            None,
            true,
            checkpoint::CaptureScope::DeclaredResources
        )
        .is_err()
    );
    assert!(!output.exists());
    let mut config = serde_json::to_value(manifest).unwrap();
    config["transfer"]["data"]["files"]["path"] = serde_json::json!(".git/hooks");
    assert!(
        serde_json::from_value::<Manifest>(config)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn optional_missing_resource_is_recorded_and_restore_does_not_invent_it() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer: {data: {files: {location: source, path: storage/optional, required: false, snapshot: {method: quiescent-files}}}}\n";
    let (source, manifest) = fixture(&temp, policy);
    let output = temp.path().join("checkpoint");
    let checkpoint = checkpoint::create(
        &source,
        &manifest,
        &output,
        None,
        true,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    assert!(!checkpoint.resources[0].captured);
    let result = checkpoint::restore(&output, &temp.path().join("restored")).unwrap();
    assert!(!result.source.join("storage/optional").exists());
}

#[test]
fn forged_resource_metadata_cannot_write_outside_or_override_policy() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, "transfer: {}\n");
    let output = temp.path().join("checkpoint");
    checkpoint::create(
        &source,
        &manifest,
        &output,
        None,
        false,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    let path = output.join("checkpoint.json");
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["resources"] = serde_json::json!([{"name": "escape", "location": "source", "path": "../outside", "captured": false}]);
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(checkpoint::restore(&output, &temp.path().join("restored")).is_err());
    assert!(!temp.path().join("outside").exists());
    config["resources"][0]["path"] = serde_json::json!("undeclared");
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    assert!(checkpoint::restore(&output, &temp.path().join("restored")).is_err());
    assert!(!temp.path().join("restored").exists());
}

#[test]
fn atomic_publish_preserves_an_empty_destination_created_after_preflight() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("staged");
    let destination = temp.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("payload"), "retain staged data").unwrap();
    fs::create_dir(&destination).unwrap();
    assert!(parcela::private::publish_directory(&source, &destination).is_err());
    assert!(source.join("payload").exists());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
}

#[test]
fn explicit_source_only_checkpoint_does_not_invent_required_data() {
    let temp = TempDir::new().unwrap();
    let (source, manifest) = fixture(&temp, POLICY);
    let output = temp.path().join("checkpoint");
    let captured = checkpoint::create(
        &source,
        &manifest,
        &output,
        None,
        false,
        checkpoint::CaptureScope::Source,
    )
    .unwrap();
    assert!(captured.resources.iter().all(|resource| !resource.captured));
    let restored = checkpoint::restore(&output, &temp.path().join("restored")).unwrap();
    assert_eq!(restored.scope, checkpoint::CaptureScope::Source);
    assert!(!restored.source.join("storage/app.db").exists());
}

#[test]
fn selected_resource_output_overlap_is_refused_before_staging() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer:\n  data:\n    uploads:\n      location: runtime\n      path: uploads\n      snapshot: {method: quiescent-files}\n";
    let (source, manifest) = fixture(&temp, policy);
    let runtime = temp.path().join("runtime");
    fs::create_dir_all(runtime.join("uploads/nested")).unwrap();
    fs::write(runtime.join("uploads/retained"), "persistent upload").unwrap();
    let alias = temp.path().join("uploads-alias");
    symlink(runtime.join("uploads"), &alias).unwrap();
    for output in [
        runtime.join("uploads/checkpoint"),
        runtime.join("uploads/nested/checkpoint"),
        alias.join("checkpoint"),
    ] {
        let error = checkpoint::create(
            &source,
            &manifest,
            &output,
            Some(&runtime),
            true,
            checkpoint::CaptureScope::DeclaredResources,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("checkpoint output/staging overlaps selected resource uploads"),
            "{error:#}"
        );
        assert!(!output.exists());
        assert_eq!(
            fs::read_to_string(runtime.join("uploads/retained")).unwrap(),
            "persistent upload"
        );
        assert_eq!(fs::read_dir(runtime.join("uploads")).unwrap().count(), 2);
        assert_eq!(
            fs::read_dir(runtime.join("uploads/nested"))
                .unwrap()
                .count(),
            0
        );
    }
    let output = runtime.join("checkpoint");
    let captured = checkpoint::create(
        &source,
        &manifest,
        &output,
        Some(&runtime),
        true,
        checkpoint::CaptureScope::DeclaredResources,
    )
    .unwrap();
    assert!(captured.resources[0].captured);
    assert_eq!(
        fs::read_to_string(output.join("resources/uploads/payload/retained")).unwrap(),
        "persistent upload"
    );
    let source_only = checkpoint::create(
        &source,
        &manifest,
        &runtime.join("uploads/source-only"),
        None,
        false,
        checkpoint::CaptureScope::Source,
    )
    .unwrap();
    assert!(!source_only.resources[0].captured);
}
