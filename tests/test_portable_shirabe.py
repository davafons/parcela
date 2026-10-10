import contextlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location("portable_shirabe", SCRIPTS / "check_portable_shirabe.py")
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class PortableShirabeTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="portable-shirabe-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.root.chmod(0o700)
        self.database = self.root / "snapshot.sqlite3"
        with contextlib.closing(sqlite3.connect(self.database)) as db, db:
            db.execute("CREATE TABLE records(value TEXT)")
            db.execute("INSERT INTO records VALUES ('retained')")
        self.database.chmod(0o600)

    def test_inspection_is_repeatable_and_does_not_create_sidecars(self):
        before = self.database.read_bytes()
        first = acceptance.inspect_database(self.database)
        self.assertEqual(first, acceptance.inspect_database(self.database))
        self.assertTrue(first["sqlite_valid"])
        self.assertEqual(first["schema_objects"], 1)
        self.assertEqual(self.database.read_bytes(), before)
        self.assertEqual([path.name for path in self.root.iterdir()], ["snapshot.sqlite3"])

    def test_live_sidecars_and_wal_mode_are_refused_without_mutation(self):
        sidecar = Path(str(self.database) + "-wal")
        sidecar.write_bytes(b"do not consume live WAL")
        with self.assertRaisesRegex(ValueError, "sidecars"):
            acceptance.inspect_database(self.database)
        self.assertEqual(sidecar.read_bytes(), b"do not consume live WAL")
        sidecar.unlink()
        with contextlib.closing(sqlite3.connect(self.database)) as db, db:
            db.execute("PRAGMA journal_mode=WAL")
        with self.assertRaisesRegex(ValueError, "rollback-journal"):
            acceptance.inspect_database(self.database)

    def test_stopped_queue_allows_only_journal_metadata_changes_without_mutation(self):
        queue = self.root / "queue.sqlite3"
        queue.write_bytes(self.database.read_bytes())
        queue.chmod(0o600)
        before = acceptance.inspect_stopped_queue(queue)
        with contextlib.closing(sqlite3.connect(queue)) as db:
            self.assertEqual(db.execute("PRAGMA journal_mode=WAL").fetchone()[0], "wal")
        retained = self.database.read_bytes()
        stopped = queue.read_bytes()
        self.assertNotEqual(retained, stopped)
        self.assertEqual(before, acceptance.inspect_stopped_queue(queue))
        self.assertEqual(queue.read_bytes(), stopped)
        self.assertEqual(self.database.read_bytes(), retained)
        self.assertEqual(set(path.name for path in self.root.iterdir()), {"snapshot.sqlite3", "queue.sqlite3"})
        with self.assertRaisesRegex(ValueError, "rollback-journal"):
            acceptance.inspect_database(queue)

    def test_queue_record_changes_are_detected_even_with_the_same_row_count(self):
        before = acceptance.inspect_stopped_queue(self.database)
        with contextlib.closing(sqlite3.connect(self.database)) as db, db:
            db.execute("UPDATE records SET value = 'changed'")
        self.assertNotEqual(before, acceptance.inspect_stopped_queue(self.database))

    def test_queue_schema_and_application_metadata_changes_are_detected(self):
        before = acceptance.inspect_stopped_queue(self.database)
        with contextlib.closing(sqlite3.connect(self.database)) as db:
            db.execute("PRAGMA user_version=1")
        after = acceptance.inspect_stopped_queue(self.database)
        self.assertNotEqual(before, after)
        with contextlib.closing(sqlite3.connect(self.database)) as db:
            db.execute("CREATE TABLE other_records(value TEXT)")
        self.assertNotEqual(after, acceptance.inspect_stopped_queue(self.database))

    def test_stopped_queue_refuses_wal_sidecars_without_consuming_them(self):
        with contextlib.closing(sqlite3.connect(self.database)) as db, db:
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("UPDATE records SET value = 'uncheckpointed'")
            sidecar = Path(str(self.database) + "-wal")
            before = sidecar.read_bytes()
            with self.assertRaisesRegex(ValueError, "sidecars"):
                acceptance.inspect_stopped_queue(self.database)
            self.assertEqual(sidecar.read_bytes(), before)

    def test_queue_shutdown_requires_an_unambiguous_no_openers_result(self):
        for result in (
            subprocess.CompletedProcess([], 0, "123\n", ""),
            subprocess.CompletedProcess([], 2, "", "failed"),
            subprocess.CompletedProcess([], 1, "", "cannot inspect"),
        ):
            with self.subTest(result=result), patch.object(acceptance.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(ValueError, "openers"):
                    acceptance.require_queue_stopped(self.database, {}, self.root)
        with patch.object(acceptance.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "")):
            acceptance.require_queue_stopped(self.database, {}, self.root)

    def test_symlinks_and_public_snapshots_fail_closed(self):
        linked = self.root / "linked.sqlite3"
        linked.symlink_to(self.database)
        with self.assertRaisesRegex(ValueError, "regular file"):
            acceptance.inspect_database(linked)
        self.database.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "private"):
            acceptance.inspect_database(self.database)
        self.root.chmod(0o755)
        with self.assertRaisesRegex(ValueError, "private"):
            acceptance.private_directory(self.root)
        self.root.chmod(0o700)

    def test_overlay_does_not_start_jobs_or_sdk_and_preserves_captured_manifest(self):
        manifest = {
            "project": {"id": "shirabe", "name": "shirabe"},
            "environment": {"SHIRABE_RUN_JOBS": "1", "GOOGLE_CLIENT_SECRET": "must not inherit"},
            "tasks": {"uploads-snapshot": {"argv": ["bin/portable-uploads", "capture"]}},
            "process_compose": {
                "version": "0.5",
                "processes": {name: {"command": "arbitrary original command"}
                              for name in ("web", "css", "jobs", "sdk", "other-writer")},
            },
        }
        before = json.dumps(manifest)
        reviewed = acceptance.review_manifest(manifest)
        self.assertEqual(set(reviewed["process_compose"]["processes"]), {"web", "css"})
        self.assertEqual(reviewed["environment"]["SHIRABE_RUN_JOBS"], "0")
        self.assertEqual(reviewed["environment"]["SOLID_QUEUE_SKIP_RECURRING"], "true")
        self.assertNotIn("GOOGLE_CLIENT_SECRET", reviewed["environment"])
        self.assertEqual(reviewed["tasks"]["uploads-snapshot"], manifest["tasks"]["uploads-snapshot"])
        self.assertIn("verify-replica", reviewed["tasks"])
        self.assertEqual(json.dumps(manifest), before)

    def test_failed_commands_preserve_output_before_raising(self):
        argv = [sys.executable, "-c", "import sys; print('probe output'); print('actual failure', file=sys.stderr); sys.exit(7)"]
        with self.assertRaises(subprocess.CalledProcessError):
            acceptance.checked_command(argv, {}, self.root)
        log = self.root / "failed-command.json"
        result = json.loads(log.read_text())
        self.assertEqual(result["exit_code"], 7)
        self.assertEqual(result["stdout"], "probe output\n")
        self.assertEqual(result["stderr"], "actual failure\n")
        self.assertEqual(log.stat().st_mode & 0o777, 0o600)

    def test_timeout_preserves_partial_output_without_replaying_the_command(self):
        failure = subprocess.TimeoutExpired(["probe"], 1, output=b"partial output", stderr=b"timeout detail")
        with patch.object(acceptance.subprocess, "run", side_effect=failure) as run:
            with self.assertRaises(subprocess.TimeoutExpired):
                acceptance.checked_command(["probe"], {}, self.root, timeout=1)
        run.assert_called_once()
        result = json.loads((self.root / "failed-command.json").read_text())
        self.assertEqual(result["timeout"], 1)
        self.assertEqual(result["stdout"], "partial output")
        self.assertEqual(result["stderr"], "timeout detail")

    def test_inline_ruby_probe_is_ascii_safe_under_non_utf8_locales(self):
        self.assertTrue(acceptance.PROBE.isascii(), "Rails runner argv must not contain locale-dependent source literals")

    def test_tokenizer_requires_one_private_pinned_full_edition(self):
        root = self.root / "tokenizer"
        root.mkdir(mode=0o700)
        version = root / "sudachi-dictionary-20260723"
        version.mkdir(mode=0o700)
        dictionary = version / "system_full.dic"
        dictionary.write_bytes(b"tokenizer asset fixture")
        dictionary.chmod(0o600)
        receipt = acceptance.inspect_tokenizer(root, "20260723")
        self.assertEqual(receipt["path"], str(dictionary))
        self.assertEqual(receipt["edition"], "full")
        self.assertEqual(receipt["sha256"], acceptance.digest(dictionary))
        with self.assertRaisesRegex(ValueError, "eight-digit"):
            acceptance.inspect_tokenizer(root, "../outside")
        extra = version / "system_core.dic"
        extra.write_bytes(b"wrong edition")
        with self.assertRaisesRegex(ValueError, "only the selected"):
            acceptance.inspect_tokenizer(root, "20260723")
        extra.unlink()
        dictionary.unlink()
        dictionary.symlink_to(self.database)
        with self.assertRaisesRegex(ValueError, "regular file"):
            acceptance.inspect_tokenizer(root, "20260723")

    def test_explicit_tokenizer_binding_survives_private_home_isolation(self):
        manifest = {
            "project": {"id": "shirabe"},
            "environment": {
                "KABOSU_DICT_DIR": "/private/task/tokenizer",
                "KABOSU_DICT_VERSION": "20260723",
                "GOOGLE_CLIENT_SECRET": "do not preserve",
            },
            "process_compose": {"version": "0.5", "processes": {"web": {}, "css": {}}},
        }
        reviewed = acceptance.review_manifest(manifest)
        self.assertEqual(reviewed["environment"]["KABOSU_DICT_DIR"], "/private/task/tokenizer")
        self.assertEqual(reviewed["environment"]["KABOSU_DICT_VERSION"], "20260723")
        self.assertNotIn("GOOGLE_CLIENT_SECRET", reviewed["environment"])

    def test_unknown_projects_and_missing_services_are_not_guessed(self):
        for manifest in ({}, {"project": {"id": "other"}}, {"project": {"id": "shirabe"}}):
            with self.assertRaises(ValueError):
                acceptance.review_manifest(manifest)

    def test_standard_duplicate_and_occupied_ports_are_refused(self):
        for http, https in ((80, 443), (10000, 10000), (65536, 10001)):
            with self.assertRaises(ValueError):
                acceptance.validate_ports(http, https)
        with patch.object(acceptance.socket, "socket") as socket:
            socket.return_value.__enter__.return_value.bind.side_effect = OSError("occupied")
            with self.assertRaisesRegex(OSError, "occupied"):
                acceptance.validate_ports(10000, 10001)

    def test_output_cannot_overwrite_a_run_or_mutate_an_input_checkpoint(self):
        with self.assertRaisesRegex(ValueError, "new"):
            acceptance.work_directory(self.root, ())
        candidate = self.root / "inside-snapshot"
        with self.assertRaisesRegex(ValueError, "outside"):
            acceptance.work_directory(candidate, (self.root,))
        self.assertFalse(candidate.exists())
        with self.assertRaisesRegex(ValueError, "outside"):
            acceptance.validate_state_parent(self.root, self.root, (self.root,))

    def test_low_space_and_overlong_state_paths_fail_before_copying(self):
        for name in acceptance.DATABASES:
            (self.root / f"{name}.sqlite3").write_bytes(self.database.read_bytes())
        with patch.object(acceptance.shutil, "disk_usage") as usage:
            usage.return_value.free = 0
            with self.assertRaisesRegex(ValueError, "lacks space"):
                acceptance.validate_state_parent(Path("/short/private"), self.root, ())
        with self.assertRaisesRegex(ValueError, "too long"):
            acceptance.validate_state_parent(self.root / ("long" * 30), self.root, ())

    def test_interrupts_abort_acceptance_instead_of_silently_marking_it_complete(self):
        with self.assertRaisesRegex(InterruptedError, "signal 15"):
            acceptance.interrupted(15, None)

    def test_receipts_are_private_and_updated_without_leftover_temporary_files(self):
        path = self.root / "report.json"
        acceptance.save(path, {"complete": False})
        acceptance.save(path, {"complete": True})
        self.assertEqual(json.loads(path.read_text()), {"complete": True})
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertFalse(path.with_suffix(".tmp").exists())


if __name__ == "__main__":
    unittest.main()
