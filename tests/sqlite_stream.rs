use parcela::sqlite_stream;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

fn receiver(output: &Path, maximum: u64) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_parcela"));
    command
        .args(["transfer", "receive-sqlite", "--output"])
        .arg(output)
        .arg("--max-bytes")
        .arg(maximum.to_string());
    command
}

fn private(temp: &TempDir) {
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn streamed_online_backup_preserves_wal_fts_blobs_and_nul_text() {
    let temp = TempDir::new().unwrap();
    private(&temp);
    let source = temp.path().join("source.sqlite3");
    let db = Connection::open(&source).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE records(id INTEGER PRIMARY KEY, text TEXT, data BLOB); CREATE VIRTUAL TABLE search USING fts5(text); INSERT INTO search VALUES ('日本語 dictionary');").unwrap();
    db.execute(
        "INSERT INTO records VALUES(1, ?, ?)",
        rusqlite::params!["embedded\0nul", &[0u8, 255, 42][..]],
    )
    .unwrap();
    db.execute(
        "INSERT INTO records VALUES(2, 'large', zeroblob(6000000))",
        [],
    )
    .unwrap();
    let output = temp.path().join("snapshot.sqlite3");
    let result =
        sqlite_stream::backup_over_command(&source, &mut receiver(&output, 16 * 1024 * 1024))
            .unwrap();
    assert_eq!(result.activation, "paused; no ownership transfer");
    let bytes = fs::read(&output).unwrap();
    assert_eq!(result.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(result.bytes, bytes.len() as u64);
    let restored = Connection::open(&output).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT text FROM records WHERE id=1", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "embedded\0nul"
    );
    assert_eq!(
        restored
            .query_row("SELECT data FROM records WHERE id=1", [], |row| row
                .get::<_, Vec<u8>>(0))
            .unwrap(),
        vec![0, 255, 42]
    );
    assert_eq!(
        restored
            .query_row("SELECT length(data) FROM records WHERE id=2", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        6_000_000
    );
    assert_eq!(
        restored
            .query_row(
                "SELECT count(*) FROM search WHERE search MATCH '日本語'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".parcela-sqlite-stream-")
    }));
}

#[test]
fn source_read_transaction_pins_snapshot_while_another_writer_commits() {
    let temp = TempDir::new().unwrap();
    private(&temp);
    let source = temp.path().join("source.sqlite3");
    let db = Connection::open(&source).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE records(value TEXT); INSERT INTO records VALUES ('before');").unwrap();
    let output = temp.path().join("snapshot.sqlite3");
    let sentinel = temp.path().join("started");
    let mut command = Command::new("python3");
    command.arg("-c").arg("import pathlib, sqlite3, subprocess, sys; pathlib.Path(sys.argv[3]).write_text('started'); c=sqlite3.connect(sys.argv[2]); c.execute(\"INSERT INTO records VALUES ('during')\"); c.commit(); c.close(); sys.exit(subprocess.call(sys.argv[4:]))")
        .arg("unused").arg(&source).arg(&sentinel).arg(env!("CARGO_BIN_EXE_parcela"))
        .args(["transfer", "receive-sqlite", "--output"]).arg(&output).args(["--max-bytes", "1048576"]);
    sqlite_stream::backup_over_command(&source, &mut command).unwrap();
    assert!(sentinel.exists());
    let restored = Connection::open(output).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM records", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM records", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn receiver_rejects_corruption_and_does_not_publish_partial_files() {
    let temp = TempDir::new().unwrap();
    private(&temp);
    let output = temp.path().join("snapshot.sqlite3");
    let mut child = receiver(&output, 4096)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut response = child.stdout.take().unwrap();
    let mut ready = [1];
    response.read_exact(&mut ready).unwrap();
    assert_eq!(ready, [0]);
    let mut input = child.stdin.take().unwrap();
    input.write_all(&[1]).unwrap();
    input.write_all(&0u64.to_le_bytes()).unwrap();
    input.write_all(&4u64.to_le_bytes()).unwrap();
    input.write_all(&[0u8; 32]).unwrap();
    input.write_all(b"oops").unwrap();
    drop(input);
    assert!(!child.wait().unwrap().success());
    assert!(!output.exists());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn interruption_and_size_violation_preserve_source_and_existing_destination() {
    let temp = TempDir::new().unwrap();
    private(&temp);
    let source = temp.path().join("source.sqlite3");
    let db = Connection::open(&source).unwrap();
    db.execute_batch(
        "CREATE TABLE records(value TEXT); INSERT INTO records VALUES ('retain source');",
    )
    .unwrap();
    let output = temp.path().join("snapshot.sqlite3");
    assert!(sqlite_stream::backup_over_command(&source, &mut receiver(&output, 1)).is_err());
    assert!(!output.exists());
    fs::write(&output, b"unrelated destination").unwrap();
    assert!(
        sqlite_stream::backup_over_command(&source, &mut receiver(&output, 1024 * 1024)).is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"unrelated destination");
    assert_eq!(
        db.query_row("SELECT value FROM records", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "retain source"
    );
    assert!(sqlite_stream::backup_over_command(&source, &mut Command::new("false")).is_err());
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".parcela-sqlite-stream-")
    }));
}
