import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from scripts.build_release_source import ROOT, SUPPLEMENTS, check_source_path, frozen_files, render_notices


class ReleaseSourcePaths(unittest.TestCase):
    def test_hidden_skill_and_release_inputs_are_allowed(self):
        for name in ['.agents/skills/parcela/SKILL.md', 'Cargo.lock', 'scripts/build_release_source.py']:
            check_source_path(name)

    def test_generated_private_and_escaping_paths_are_refused(self):
        for name in ['', '/outside', '../outside', 'src/../outside', './src/main.rs',
                     'src//main.rs', 'src\\main.rs', 'src/line\nbreak', '.git/config', '.jj/repo',
                     '.cargo/config.toml', 'vendor/package', 'target/private', 'release-source.json',
                     'THIRD_PARTY_NOTICES.txt']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                check_source_path(name)

    def test_nonliteral_revisions_are_refused_before_running_vcs(self):
        for revision in ['@', 'main', 'a' * 39, 'g' * 40, 'a' * 40 + '|@']:
            with self.subTest(revision=revision), self.assertRaisesRegex(ValueError, 'full 40-character'):
                frozen_files(ROOT, revision)


@unittest.skipUnless(shutil.which('jj'), 'jj test prerequisite unavailable')
class FrozenRevisionExport(unittest.TestCase):
    def test_jj_and_read_only_git_export_exact_frozen_bytes_and_modes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            checkout = root / 'owner'
            jj = ['jj', '--config', 'user.name="Source test"', '--config', 'user.email="source-test@example.invalid"']
            subprocess.run([*jj, 'git', 'init', '--colocate', str(checkout)], check=True, capture_output=True)
            script = checkout / 'run.sh'
            script.write_bytes(b'printf frozen\\n')
            script.chmod(0o755)
            revision = subprocess.check_output([*jj, 'log', '-r', '@', '--no-graph', '-T', 'commit_id'],
                                               cwd=checkout).decode()
            script.write_bytes(b'preserve these newer bytes\n')
            original = frozen_files(checkout, revision)
            self.assertEqual(original['run.sh'].content, b'printf frozen\\n')
            self.assertTrue(original['run.sh'].executable)
            self.assertEqual(script.read_bytes(), b'preserve these newer bytes\n')
            reader = root / 'reader'
            reader.mkdir()
            (reader / '.git').write_text(f'gitdir: {checkout / ".git"}\n')
            self.assertFalse((reader / '.jj').exists())
            self.assertEqual(frozen_files(reader, revision), original)
            self.assertEqual(sorted(path.name for path in reader.iterdir()), ['.git'])


class ReleaseNotices(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.source = Path(temporary.name)
        self.supplement = SUPPLEMENTS[('rsqlite-vfs', '0.1.1')]
        license_path = self.source / self.supplement['path']
        license_path.parent.mkdir()
        license_path.write_bytes((ROOT / self.supplement['path']).read_bytes())
        self.vcs = self.source / 'vendor/rsqlite-vfs-0.1.1/.cargo_vcs_info.json'
        self.vcs.parent.mkdir(parents=True)
        self.vcs.write_text(json.dumps({'git': {'sha1': self.supplement['commit']}}))
        self.report = {'licenses': [{'name': 'MIT License', 'id': 'MIT',
                                   'text': 'Copyright (c) <year> <copyright holders>',
                                   'source_path': '/private/this-path-must-not-be-rendered',
                                   'used_by': [{'crate': {'name': 'rsqlite-vfs', 'version': '0.1.1'},
                                                'path': '/private/neither-must-this'}]}]}

    def test_actual_pinned_copyright_replaces_placeholder_without_local_paths(self):
        rendered = render_notices(self.report, self.source)
        self.assertIn(b'Copyright (c) 2024 Spxg', rendered)
        self.assertIn(b'rsqlite-vfs 0.1.1', rendered)
        self.assertIn(b'Permission is hereby granted, free of charge', rendered)
        self.assertNotIn(b'<year>', rendered)
        self.assertNotIn(b'/private/', rendered)

    def test_unknown_missing_mit_copyright_is_a_release_error(self):
        report = copy.deepcopy(self.report)
        report['licenses'][0]['used_by'][0]['crate']['version'] = '0.1.2'
        with self.assertRaisesRegex(ValueError, 'unresolved MIT copyright'):
            render_notices(report, self.source)

    def test_changed_supplement_is_a_release_error(self):
        (self.source / self.supplement['path']).write_text('changed')
        with self.assertRaisesRegex(ValueError, 'provenance mismatch'):
            render_notices(self.report, self.source)

    def test_different_published_crate_commit_is_a_release_error(self):
        self.vcs.write_text(json.dumps({'git': {'sha1': '0' * 40}}))
        with self.assertRaisesRegex(ValueError, 'provenance mismatch'):
            render_notices(self.report, self.source)

    def test_copyright_and_notice_files_are_preserved(self):
        (self.vcs.parent / 'COPYRIGHT').write_text('Additional contributor copyright\n')
        (self.vcs.parent / 'NOTICE.txt').write_text('Required upstream notice\n')
        rendered = render_notices(self.report, self.source)
        self.assertIn(b'vendor/rsqlite-vfs-0.1.1/COPYRIGHT\n\nAdditional contributor copyright\n', rendered)
        self.assertIn(b'NOTICE.txt\n\nRequired upstream notice\n', rendered)
