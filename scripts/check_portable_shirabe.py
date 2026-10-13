"""Opt-in destination acceptance of a retained Shirabe checkpoint and paused data copies."""

import argparse
import contextlib
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import signal
import shutil
import socket
import sqlite3
import ssl
import subprocess
import tempfile
from urllib.parse import quote

from check_live import wait_for

DATABASES = {
    "primary": "development.sqlite3",
    "dictionary": "development_dictionary.sqlite3",
    "queue": "development_queue.sqlite3",
    "cache": "development_cache.sqlite3",
}
PROBE = """
require 'digest'
raise 'unsafe queue adapter' unless ActiveJob::Base.queue_adapter.is_a?(ActiveJob::QueueAdapters::TestAdapter)
raise 'unsafe mail delivery' unless ActionMailer::Base.delivery_method == :test
blob = ActiveStorage::Blob.order(:id).first
raise 'no captured upload to exercise' unless blob
bytes = blob.download
raise 'upload checksum mismatch' unless Digest::MD5.base64digest(bytes) == blob.checksum
query = "\\u732b"
tokens = Tokenizer.tokenize(query).map(&:surface)
raise 'tokenizer did not read Japanese' unless tokens == [query]
puts 'PARCELA_PROBE=' + {
  tokenizer_path: Kabosu::Dictionary.path,
  tokenizer_surfaces: tokens,
  database_directory: ENV.fetch('SHIRABE_DATABASE_DIRECTORY'),
  uploads_directory: ENV.fetch('SHIRABE_UPLOADS_DIRECTORY'),
  dictionary_entries: DictionaryRecord.connection.select_value('SELECT count(*) FROM entries').to_i,
  users: User.count,
  uploads: ActiveStorage::Blob.count,
  sample_upload_sha256: Digest::SHA256.hexdigest(bytes),
  queue_adapter: ActiveJob::Base.queue_adapter.class.name,
  mail_delivery: ActionMailer::Base.delivery_method
}.to_json
"""


def private_directory(path):
    if path.is_symlink() or not path.is_dir():
        raise ValueError(f"Expected a real private directory: {path}")
    stat = path.stat()
    if stat.st_uid != os.getuid() or stat.st_mode & 0o077:
        raise ValueError(f"Directory must be private and user-owned: {path}")
    return path.resolve(strict=True)


def work_directory(path, inputs):
    if path.exists() or path.is_symlink():
        raise ValueError("Work directory must be new; acceptance never overwrites an existing run")
    parent = private_directory(path.parent)
    work = parent / path.name
    if any(work == root or root in work.parents for root in inputs):
        raise ValueError("Work directory must be outside the retained input snapshots")
    return work


def validate_state_parent(parent, databases, inputs):
    if any(parent == root or root in parent.parents for root in inputs):
        raise ValueError("State directory must be outside retained input snapshots")
    socket_path = parent / "ph-xxxxxxxx" / "environments" / ("0" * 36) / "pc.sock"
    if len(os.fsencode(socket_path)) >= 100:
        raise ValueError("State parent is too long for the Process Compose Unix socket")
    required = sum((databases / f"{name}.sqlite3").stat().st_size for name in DATABASES) + 512 * 1024 * 1024
    if shutil.disk_usage(parent).free < required:
        raise ValueError("State filesystem lacks space for independent database copies and upload scratch")


def digest(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Expected a regular file: {path}")
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def inspect_tokenizer(root, version):
    if not re.fullmatch(r"[0-9]{8}", version):
        raise ValueError("Tokenizer version must be an explicit eight-digit Sudachi release")
    root = private_directory(root)
    directory = private_directory(root / f"sudachi-dictionary-{version}")
    path = directory / "system_full.dic"
    if set(root.glob("sudachi-dictionary-*/system_*.dic")) != {path}:
        raise ValueError("Tokenizer directory must contain only the selected full-edition dictionary")
    stat = path.lstat()
    if path.is_symlink() or not path.is_file() or stat.st_uid != os.getuid() or stat.st_mode & 0o077:
        raise ValueError("Tokenizer dictionary must be a private user-owned regular file")
    return {"path": str(path), "version": version, "edition": "full", "bytes": stat.st_size, "sha256": digest(path)}


@contextlib.contextmanager
def inspected_database(path, *, stopped_queue=False):
    stat = path.lstat()
    if path.is_symlink() or not path.is_file() or stat.st_uid != os.getuid() or stat.st_mode & 0o077:
        raise ValueError(f"Snapshot must be a private user-owned regular file: {path}")
    if any(Path(str(path) + suffix).exists() or Path(str(path) + suffix).is_symlink()
           for suffix in ("-wal", "-shm", "-journal")):
        raise ValueError("Use a finalized snapshot without SQLite sidecars, not a live database")
    with path.open("rb") as source:
        header = source.read(20)
    modes = (b"\x01\x01", b"\x02\x02") if stopped_queue else (b"\x01\x01",)
    if header[:16] != b"SQLite format 3\x00" or header[18:20] not in modes:
        raise ValueError("Snapshot must be normalized to rollback-journal mode")
    with contextlib.closing(sqlite3.connect(path.resolve().as_uri() + "?mode=ro&immutable=1", uri=True)) as db:
        if db.execute("PRAGMA quick_check(1)").fetchone()[0] != "ok":
            raise ValueError(f"Invalid SQLite snapshot: {path}")
        yield db


def inspect_database(path):
    with inspected_database(path) as db:
        schema_objects = db.execute("SELECT count(*) FROM sqlite_schema").fetchone()[0]
    return {"bytes": path.stat().st_size, "sha256": digest(path), "sqlite_valid": True,
            "schema_objects": schema_objects}


def inspect_stopped_queue(path):
    with inspected_database(path, stopped_queue=True):
        with path.open("rb") as source:
            header = source.read(100)
            checksum = hashlib.sha256(header[:18] + header[20:24] + header[28:92] + header[96:])
            while chunk := source.read(1024 * 1024):
                checksum.update(chunk)
    return {"bytes": path.stat().st_size, "sha256_excluding_journal_metadata": checksum.hexdigest(),
            "sqlite_valid": True}


def require_queue_stopped(path, environment, work):
    result = checked_command(["lsof", "-t", "--", str(path)], environment, work, timeout=15, check=False)
    if result.returncode != 1 or result.stdout or result.stderr:
        raise ValueError("Could not establish that the copied queue has no openers")


def review_manifest(manifest):
    if manifest.get("project", {}).get("id") != "shirabe":
        raise ValueError("This acceptance recipe is only for Shirabe")
    graph = manifest.get("process_compose", {})
    if not all(name in graph.get("processes", {}) for name in ("web", "css")):
        raise ValueError("Captured project must declare web and CSS services")
    result = json.loads(json.dumps(manifest))
    result["environment"] = {
        "RAILS_ENV": "development",
        "SHIRABE_PREVIEW": "1",
        "SHIRABE_RUN_JOBS": "0",
        "SOLID_QUEUE_SKIP_RECURRING": "true",
        "BOOTSNAP_CACHE_DIR": "${PARCELA_RUNTIME_DIR}/cache/bootsnap",
        "TMPDIR": "${PARCELA_RUNTIME_DIR}/tmp",
        "PIDFILE": "${PARCELA_RUNTIME_DIR}/tmp/pids/server.pid",
        "PUMA_ENABLE_METRICS": "0",
    }
    for key in ("KABOSU_DICT_DIR", "KABOSU_DICT_VERSION"):
        if key in manifest.get("environment", {}):
            result["environment"][key] = manifest["environment"][key]
    result["inherit_environment"] = True
    result["process_compose"] = {
        "version": graph["version"],
        "processes": {name: {"command": command, "availability": {"restart": "no"}}
                      for name, command in {
                          "web": "bin/rails server -b 127.0.0.1",
                          "css": "bin/rails tailwindcss:watch[always]",
                      }.items()},
    }
    result.setdefault("tasks", {})["verify-replica"] = {
        "argv": ["bin/rails", "runner", PROBE], "timeout_seconds": 180,
    }
    return result


def validate_ports(http, https):
    if http == https or not all(1024 <= port <= 65535 for port in (http, https)):
        raise ValueError("Acceptance requires distinct explicit unprivileged HTTP/HTTPS ports")
    for port in (http, https):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", port))


def save(path, value):
    temporary = path.with_suffix(".tmp")
    with temporary.open("x") as output:
        json.dump(value, output, indent=2)
        output.flush()
        os.fsync(output.fileno())
    temporary.chmod(0o600)
    os.replace(temporary, path)


def checked_command(argv, environment, work, timeout=180, check=True):
    try:
        result = subprocess.run(argv, capture_output=True, text=True, env=environment, check=False, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        def text(value):
            return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""
        save(work / "failed-command.json", {"argv": list(argv), "timeout": error.timeout,
                                            "stdout": text(error.stdout), "stderr": text(error.stderr)})
        raise
    if check and result.returncode:
        save(work / "failed-command.json", {"argv": list(argv), "exit_code": result.returncode,
                                            "stdout": result.stdout, "stderr": result.stderr})
        result.check_returncode()
    return result


def interrupted(signum, _frame):
    raise InterruptedError(f"Acceptance interrupted by signal {signum}")


def processes(process_compose, runtime, log_path, environment):
    output = subprocess.check_output([process_compose, "--use-uds", "--unix-socket", str(runtime / "pc.sock"),
                                      "--token-file", str(runtime / "pc.token"), "--log-file", str(log_path), "process", "list", "--output", "json"],
                                     env=environment, text=True, stderr=subprocess.DEVNULL, timeout=15)
    records = json.loads(output)
    if isinstance(records, dict):
        records = records.get("data", records.get("processes", records))
    if not isinstance(records, list):
        raise ValueError("Unexpected Process Compose process list")
    selected = {record["name"]: record["status"] for record in records}
    if set(selected) != {"web", "css"}:
        raise ValueError(f"Unexpected graph processes: {selected}")
    return selected if all(status == "Running" for status in selected.values()) else False


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--parcela", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--database-dir", type=Path, required=True)
    parser.add_argument("--uploads-snapshot", type=Path, required=True)
    parser.add_argument("--tokenizer-dir", type=Path, required=True)
    parser.add_argument("--tokenizer-version", required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--state-parent", type=Path, required=True)
    parser.add_argument("--caddy", type=Path, required=True)
    parser.add_argument("--process-compose", type=Path, required=True)
    parser.add_argument("--http-port", type=int, required=True)
    parser.add_argument("--https-port", type=int, required=True)
    args = parser.parse_args()
    validate_ports(args.http_port, args.https_port)
    checkpoint = private_directory(args.checkpoint)
    databases = private_directory(args.database_dir)
    uploads = private_directory(args.uploads_snapshot)
    tokenizer = private_directory(args.tokenizer_dir)
    tokenizer_receipt = inspect_tokenizer(tokenizer, args.tokenizer_version)
    inputs = (checkpoint, databases, uploads, tokenizer)
    work = work_directory(args.work_dir, inputs)
    state_parent = private_directory(args.state_parent)
    validate_state_parent(state_parent, databases, inputs)
    work.mkdir(mode=0o700)
    state = Path(tempfile.mkdtemp(prefix="ph-", dir=state_parent)).resolve()
    (work / "home").mkdir(mode=0o700)
    environment = {key: value for key, value in os.environ.items()
                   if key in ("PATH", "LANG", "LC_ALL", "BUNDLE_PATH", "BUNDLE_APP_CONFIG", "BUNDLE_USER_HOME")}
    environment.update(HOME=str(work / "home"), BUNDLE_FROZEN="true")
    binary = str(args.parcela.resolve(strict=True))
    process_compose = str(args.process_compose.resolve(strict=True))
    report = {"version": 1, "complete": False, "work_dir": str(work), "state_dir": str(state),
              "activation": "acceptance only; no execution ownership transfer",
              "data_consistency": "independently consistent snapshots; not an atomic application checkpoint",
              "original_transport_receipt_available": False,
              "tokenizer": tokenizer_receipt,
              "test_ports": {"http": args.http_port, "https": args.https_port}}
    children, logs = [], []

    def cli(source, *argv, check=True, timeout=180):
        return checked_command([binary, "--path", str(source), "--state-dir", str(state), *map(str, argv)],
                               environment, work, timeout=timeout, check=check)

    def spawn(source, *argv):
        log = (work / f"child-{len(children)}.log").open("x")
        logs.append(log)
        child = subprocess.Popen([binary, "--path", str(source), "--state-dir", str(state), *map(str, argv)],
                                 env=environment, stdout=log, stderr=log, stdin=subprocess.DEVNULL)
        children.append(child)
        return child

    def request(host, path):
        context = ssl.create_default_context(cafile=str(state / "proxy/data/pki/authorities/local/root.crt"))
        with context.wrap_socket(socket.create_connection(("127.0.0.1", args.https_port), timeout=20),
                                 server_hostname=host) as connection:
            connection.sendall(f"GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: Mozilla/5.0 Firefox/128.0\r\n\r\n".encode())
            response = http.client.HTTPResponse(connection)
            response.begin()
            body = response.read()
            return {"status": response.status, "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}

    handlers = {signum: signal.signal(signum, interrupted) for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
    try:
        save(work / "report.json", report)
        report["snapshots"] = {}
        for name in DATABASES:
            print(f"Validating retained {name} snapshot", flush=True)
            report["snapshots"][name] = inspect_database(databases / f"{name}.sqlite3")
            save(work / "report.json", report)
        print("Restoring independent source and private resource copies", flush=True)
        restored = json.loads(cli(work, "transfer", "restore", args.checkpoint, "--destination", work / "replica").stdout)
        source = Path(restored["source"])
        report["restore"] = restored
        manifest = source / "parcela.yaml"
        captured_manifest = manifest.read_bytes()
        (work / "captured-parcela.yaml").write_bytes(captured_manifest)
        parsed = checked_command(["ruby", "-rjson", "-ryaml", "-e",
                                  "puts JSON.generate(YAML.safe_load(File.read(ARGV[0])))", str(manifest)],
                                 environment, work, timeout=15).stdout
        parsed = json.loads(parsed)
        parsed.setdefault("environment", {}).update(
            KABOSU_DICT_DIR=str(tokenizer), KABOSU_DICT_VERSION=args.tokenizer_version,
        )
        manifest.write_text(json.dumps(review_manifest(parsed), indent=2))
        report["test_overlay"] = {"captured_manifest_sha256": hashlib.sha256(captured_manifest).hexdigest(),
                                  "review_manifest_sha256": digest(manifest), "services": ["web", "css"],
                                  "excluded_services": ["jobs", "sdk"], "source_checkpoint_unchanged": True}
        cli(source, "config", "check")
        adopted = json.loads(cli(source, "workspace", "adopt", "--role", "preview", "--label", "tower").stdout)
        workspace = adopted["workspace"]
        runtime = Path(adopted["runtime_dir"])
        runtime.mkdir(mode=0o700, parents=True)
        data = runtime / "data"
        data.mkdir(mode=0o700)
        for name, filename in DATABASES.items():
            destination = data / filename
            checked_command(["cp", "--reflink=auto", "--", str(databases / f"{name}.sqlite3"), str(destination)],
                            environment, work)
            destination.chmod(0o600)
            if digest(destination) != report["snapshots"][name]["sha256"]:
                raise ValueError(f"Replica database differs from snapshot: {name}")
        report["queue_before"] = inspect_stopped_queue(data / DATABASES["queue"])
        checked_command(["python3", str(source / "bin/portable-uploads"), "restore", "--snapshot", str(uploads),
                         "--storage", str(runtime / "uploads")], environment, work)
        report["uploads_manifest_sha256"] = digest(uploads / "uploads.json")
        inventory = json.loads((uploads / "uploads.json").read_text())["files"]
        report["uploads"] = []
        for entry in inventory:
            expected = digest(uploads / entry["path"])
            if digest(runtime / "uploads" / entry["path"]) != expected:
                raise ValueError("Restored upload differs from its captured bytes")
            report["uploads"].append({"path": entry["path"], "bytes": entry["bytes"], "sha256": expected})
        report["workspace"] = workspace
        report["runtime_dir"] = str(runtime)
        save(work / "report.json", report)
        proxy = spawn(work, "proxy", "run", "--caddy", args.caddy.resolve(strict=True), "--test-profile",
                      "--http-port", args.http_port, "--https-port", args.https_port)
        wait_for(lambda: cli(work, "proxy", "status", check=False).returncode == 0, [proxy], timeout=30)
        print("Starting isolated web/CSS acceptance; copied jobs remain disabled", flush=True)
        app = spawn(source, "up", "--process-compose", process_compose, "--readiness-timeout", "180")
        report["processes"] = wait_for(lambda: processes(process_compose, runtime, work / "pc-client.log", environment), [proxy, app], timeout=180)
        host = workspace["hostname"]
        report["health"] = wait_for(lambda: (value if (value := request(host, "/up"))["status"] == 200 else False),
                                    [proxy, app], timeout=180)
        probe = cli(source, "task", "verify-replica", timeout=200).stdout
        line = next((line for line in probe.splitlines() if line.startswith("PARCELA_PROBE=")), None)
        if line is None:
            raise ValueError("Rails did not report a replica probe")
        report["rails"] = json.loads(line.removeprefix("PARCELA_PROBE="))
        if report["rails"]["database_directory"] != str(data) or report["rails"]["uploads_directory"] != str(runtime / "uploads"):
            raise ValueError("Rails resource paths did not bind to the replica")
        if report["rails"]["tokenizer_path"] != tokenizer_receipt["path"]:
            raise ValueError("Rails tokenizer did not bind to the declared dependency")
        if report["rails"]["dictionary_entries"] <= 0:
            raise ValueError("Replica has no searchable dictionary corpus")
        if report["rails"]["uploads"] != len(report["uploads"]):
            raise ValueError("Upload inventory does not match the primary database")
        if report["rails"]["sample_upload_sha256"] not in {entry["sha256"] for entry in report["uploads"]}:
            raise ValueError("Rails did not read a captured upload")
        report["search"] = request(host, "/en/search/" + quote("猫"))
        if report["search"]["status"] != 200:
            raise ValueError(f"Search request failed: {report['search']}")
        app.send_signal(signal.SIGTERM)
        report["app_exit"] = app.wait(timeout=15)
        report["stopped_route"] = request(host, "/up")
        if report["stopped_route"]["status"] != 503:
            raise ValueError("Stopped replica route was not withdrawn")
        queue = data / DATABASES["queue"]
        require_queue_stopped(queue, environment, work)
        report["queue_after"] = inspect_stopped_queue(queue)
        if report["queue_after"] != report["queue_before"]:
            raise ValueError("Copied queue changed beyond SQLite journal metadata during read-only acceptance")
        for name in DATABASES:
            if digest(databases / f"{name}.sqlite3") != report["snapshots"][name]["sha256"]:
                raise ValueError(f"Retained input snapshot changed: {name}")
        for entry in report["uploads"]:
            if digest(uploads / entry["path"]) != entry["sha256"]:
                raise ValueError("Retained upload snapshot changed")
        if digest(uploads / "uploads.json") != report["uploads_manifest_sha256"]:
            raise ValueError("Retained upload manifest changed")
        if inspect_tokenizer(tokenizer, args.tokenizer_version) != tokenizer_receipt:
            raise ValueError("Retained tokenizer dependency changed")
        report["queue_unchanged"] = True
        report["retained_snapshots_unchanged"] = True
        report["retained_uploads_unchanged"] = True
        report["complete"] = True
    except Exception as error:
        report["error"] = str(error)
        if (work / "failed-command.json").exists():
            report["failed_command"] = str(work / "failed-command.json")
        raise
    finally:
        for signum, handler in handlers.items():
            signal.signal(signum, handler)
        teardown = []
        for child in reversed(children):
            if child.poll() is None:
                child.send_signal(signal.SIGTERM)
                try:
                    child.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                    report["complete"] = False
                    report["error"] = "Owned foreground process required forced termination; inspect descendant teardown"
            teardown.append({"pid": child.pid, "exit": child.returncode})
        for log in logs:
            log.close()
        report["teardown"] = teardown
        report["activation"] = "paused after acceptance; no execution ownership transfer"
        save(work / "report.json", report)
    print(json.dumps(report, indent=2))
    if not report["complete"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
