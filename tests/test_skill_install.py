import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


DOCUMENT = Path(__file__).resolve().parents[1] / 'docs/site/agents.md'
PAYLOAD = b'---\nname: parcela\ndescription: Test fixture\n---\nFixture instructions.\n'


class ClaudeSkillInstallation(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='parcela-skill-install-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.project = self.root / 'project with spaces'
        self.project.mkdir()
        self.home = self.root / 'home'
        self.home.mkdir()
        self.state = self.root / 'state'
        self.state.mkdir()
        (self.state / 'sentinel').write_bytes(b'unchanged state')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        command = self.bin / 'parcela'
        command.write_text("#!/bin/sh\nprintf '%s' '" + PAYLOAD.decode() + "'\n")
        command.chmod(0o700)
        self.environment = {
            'PATH': str(self.bin) + os.pathsep + os.defpath,
            'HOME': str(self.home),
            'PARCELA_STATE_DIR': str(self.state),
        }
        section = DOCUMENT.read_text().split('## Install the skill for Claude Code\n', 1)[1]
        self.script = re.search(r'```sh\n(.*?)\n```', section, re.S).group(1)

    def install(self, script=None):
        result = subprocess.run(
            ['/bin/sh', '-c', self.script if script is None else script],
            cwd=self.project, env=self.environment, capture_output=True,
            text=True, timeout=5)
        self.assertEqual((self.state / 'sentinel').read_bytes(), b'unchanged state')
        self.assertEqual(list(self.state.iterdir()), [self.state / 'sentinel'])
        self.assertEqual(list(self.root.rglob('.install.*')), [])
        return result

    def refuse_redirected_ancestor(self, ancestor):
        external = self.home / '.claude'
        external.mkdir()
        redirected = self.project / ancestor
        redirected.parent.mkdir(parents=True, exist_ok=True)
        redirected.symlink_to(external, target_is_directory=True)
        result = self.install()
        self.assertNotEqual(result.returncode, 0,
                            'project installation followed an ancestor symlink into personal scope')
        self.assertEqual(list(external.iterdir()), [])
        self.assertTrue(redirected.is_symlink())
        self.assertEqual(redirected.readlink(), external)

    def test_project_root_symlink_cannot_install_a_personal_skill(self):
        self.refuse_redirected_ancestor('.claude')

    def test_skills_directory_symlink_cannot_redirect_the_installation(self):
        self.refuse_redirected_ancestor('.claude/skills')

    def test_skill_directory_symlink_cannot_redirect_the_installation(self):
        self.refuse_redirected_ancestor('.claude/skills/parcela')


if __name__ == '__main__':
    unittest.main()
