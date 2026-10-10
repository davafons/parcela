#!/usr/bin/env python3
"""Verify declared SQLite files on an SSH host without staging their payloads locally."""

import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import uuid


REMOTE_SETUP = """
import os, pathlib, stat, sys
p=pathlib.Path(sys.argv[1]); parent=p.parent.resolve(strict=True)
s=parent.stat()
if not p.is_absolute() or s.st_uid != os.getuid() or stat.S_IMODE(s.st_mode) & 0o077:
 raise SystemExit('test parent must exist and be private/user-owned')
p.mkdir(mode=0o700)
"""
REMOTE_VERIFY = """
import hashlib,json,pathlib,sqlite3,sys
p=pathlib.Path(sys.argv[1]); expected=json.loads(sys.argv[2])
with p.open('rb') as f:
 digest=hashlib.file_digest(f,'sha256').hexdigest()
if digest!=expected['sha256'] or p.stat().st_size!=expected['bytes']:
 raise SystemExit('persisted receiver bytes do not match snapshot receipt')
c=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
try:
 if c.execute('PRAGMA quick_check(1)').fetchone()[0]!='ok':
  raise SystemExit('persisted receiver SQLite quick_check failed')
 print(json.dumps({'sha256':digest,'bytes':p.stat().st_size,'sqlite_valid':True,'schema_objects':c.execute('SELECT count(*) FROM sqlite_schema').fetchone()[0]}))
finally:
 c.close()
"""


def remote(target, program, *args):
    command = shlex.join(["python3", "-c", program, *map(str, args)])
    return subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "--", target, command], check=True, capture_output=True, text=True).stdout


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--parcela", type=Path, required=True)
    parser.add_argument("--to", required=True)
    parser.add_argument("--receiver", required=True)
    parser.add_argument("--remote-dir", required=True)
    parser.add_argument("--database", action="append", required=True, metavar="NAME=PATH")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9._@:\[\]-]+", args.to) or args.to.startswith("-"):
        parser.error("--to must be one SSH destination")
    if args.report.exists() or args.report.is_symlink():
        parser.error("report already exists")
    databases = []
    names = set()
    for value in args.database:
        name, separator, path = value.partition("=")
        if not separator or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,62}", name) or name in names:
            parser.error("databases need unique DNS-style names and explicit paths")
        source = Path(path).resolve(strict=True)
        if not source.is_file():
            parser.error("database source must be a regular file")
        names.add(name)
        databases.append((name, source))
    remote(args.to, REMOTE_SETUP, args.remote_dir)
    report = {"version": 1, "id": str(uuid.uuid4()), "ssh_target": args.to, "remote_dir": args.remote_dir, "activation": "paused; no ownership transfer", "resources": [], "complete": False}
    with args.report.open("x") as log:
        for name, source in databases:
            output = str(Path(args.remote_dir) / (name + ".sqlite3"))
            print(f"Streaming {name}: {source.stat().st_size:,} main-file bytes, plus committed WAL state", flush=True)
            result = subprocess.run([str(args.parcela.resolve()), "transfer", "stream-sqlite", "--database", str(source), "--to", args.to, "--receiver", args.receiver, "--output", output], check=True, capture_output=True, text=True)
            receipt = json.loads(result.stdout)
            verified = json.loads(remote(args.to, REMOTE_VERIFY, output, json.dumps(receipt)))
            report["resources"].append({"name": name, "source": str(source), "destination": output, "snapshot": receipt, "persisted_verification": verified})
            log.seek(0)
            json.dump(report, log, indent=2)
            log.truncate()
            log.flush()
            os.fsync(log.fileno())
            print(f"Verified {name}: {verified['bytes']:,} bytes, SHA-256 {verified['sha256']}", flush=True)
        report["complete"] = True
        log.seek(0)
        json.dump(report, log, indent=2)
        log.truncate()
        log.flush()
        os.fsync(log.fileno())
    print(f"All {len(databases)} snapshots verified. No agents/services started. Receipt: {args.report}", flush=True)


if __name__ == "__main__":
    main()
