"""Build a frozen source archive and Rust notices without publishing anything."""
import argparse
from dataclasses import dataclass
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SUPPLEMENTS = {
    ('rsqlite-vfs', '0.1.1'): {
        'license': 'MIT',
        'path': 'licenses/rsqlite-vfs-0.1.1-MIT.txt',
        'sha256': 'e1c1975c3474cce31722836f101a40ffaf1cc784e4d2f38ce3e77b9712934e78',
        'commit': '1bb309784f25c401ddf95131dcde4c4cdad7aee7',
        'url': 'https://raw.githubusercontent.com/Spxg/sqlite-wasm-rs/1bb309784f25c401ddf95131dcde4c4cdad7aee7/LICENSE',
    },
}
RESERVED = {'.git', '.jj', '.cargo', 'target', 'vendor', 'THIRD_PARTY_NOTICES.txt', 'release-source.json'}


@dataclass(frozen=True)
class SourceFile:
    content: bytes
    executable: bool


def check_source_path(name):
    path = PurePosixPath(name)
    if (not path.parts or path.is_absolute() or '..' in path.parts or path.as_posix() != name
            or path.parts[0] in RESERVED or '\\' in name or any(ord(c) < 32 for c in name)):
        raise ValueError(f'unsupported source path: {name!r}')


def frozen_files(root, revision):
    if not re.fullmatch('[0-9a-f]{40}', revision):
        raise ValueError('revision must be a full 40-character commit ID')
    if (root / '.jj').exists():
        command = ['jj', '--ignore-working-copy']
        listing = subprocess.check_output([*command, 'file', 'list', '-r', revision, '-T',
                                           'path ++ "\\t" ++ file_type ++ "\\t" ++ executable ++ "\\n"'], cwd=root).decode()
        entries = []
        for line in listing.splitlines():
            name, kind, executable = line.split('\t')
            if kind != 'file' or executable not in {'true', 'false'}:
                raise ValueError(f'unsupported source entry: {name}')
            entries.append((name, executable == 'true'))
        read_command = lambda name: [*command, 'file', 'show', '-r', revision, name]
    else:
        listing = subprocess.check_output(['git', 'ls-tree', '-rz', '--full-tree', revision], cwd=root)
        entries = []
        for entry in listing.split(b'\0'):
            if not entry:
                continue
            metadata, name = entry.split(b'\t', 1)
            mode, kind, _ = metadata.split()
            if mode not in {b'100644', b'100755'} or kind != b'blob':
                raise ValueError(f'unsupported source entry: {name!r}')
            entries.append((name.decode(), mode == b'100755'))
        read_command = lambda name: ['git', 'show', f'{revision}:{name}']
    files = {}
    for name, executable in entries:
        check_source_path(name)
        files[name] = SourceFile(subprocess.check_output(read_command(name), cwd=root), executable)
    if not files:
        raise ValueError('revision contains no source files')
    return files


def render_notices(report, source):
    output = ['Parcela third-party Rust notices\n\nComponents retain their own licenses.\n']
    for record in report['licenses']:
        text = record['text']
        users = [(entry['crate']['name'], entry['crate']['version']) for entry in record['used_by']]
        for identity in users:
            if identity not in SUPPLEMENTS:
                continue
            supplement = SUPPLEMENTS[identity]
            content = (source / supplement['path']).read_bytes()
            vcs = json.loads((source / 'vendor' / '-'.join(identity) / '.cargo_vcs_info.json').read_text())
            if (hashlib.sha256(content).hexdigest() != supplement['sha256']
                    or vcs['git']['sha1'] != supplement['commit'] or record['id'] != supplement['license']
                    or users != [identity]):
                raise ValueError(f'license supplement provenance mismatch: {identity}')
            text = content.decode()
        if record['id'] == 'MIT' and ('<copyright holders>' in text or '<year>' in text):
            raise ValueError(f'unresolved MIT copyright placeholder: {users}')
        output.extend(['\n' + '=' * 72 + '\n', f"{record['name']} ({record['id']})\n\nUsed by:\n",
                       *[f'- {name} {version}\n' for name, version in users], '\n', text, '\n'])
    for path in sorted((source / 'vendor').rglob('*')):
        if path.is_file() and (path.name.upper() in {'NOTICE', 'COPYRIGHT'}
                               or path.name.upper().startswith(('NOTICE.', 'COPYRIGHT.'))):
            output.extend(['\n' + '=' * 72 + '\n', f'{path.relative_to(source)}\n\n', path.read_text(), '\n'])
    return ''.join(output).encode()


def build_source(revision, output, cargo_about):
    files = frozen_files(ROOT, revision)
    if files['scripts/build_release_source.py'].content != Path(__file__).read_bytes():
        raise ValueError('run the source builder from the selected revision')
    package = tomllib.loads(files['Cargo.toml'].content.decode())['package']
    if package['name'] != 'parcela' or not re.fullmatch('[0-9][A-Za-z0-9.+-]*', package['version']):
        raise ValueError('unsupported release package name or version')
    if subprocess.check_output([cargo_about, '--version']).decode().strip() != 'cargo-about 0.9.2':
        raise ValueError('cargo-about 0.9.2 is required')
    output.mkdir(parents=True, exist_ok=False)
    name = f"parcela-{package['version']}"
    with tempfile.TemporaryDirectory(prefix='build-', dir=output) as temporary:
        source = Path(temporary) / name
        source.mkdir()
        for path, entry in files.items():
            destination = source / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(entry.content)
            destination.chmod(0o755 if entry.executable else 0o644)
        with (output / 'build.log').open('wb') as log:
            vendor = subprocess.run(['cargo', 'vendor', '--locked', '--offline', '--versioned-dirs', 'vendor'],
                                    cwd=source, stdout=subprocess.PIPE, stderr=log, check=True, timeout=180)
            expected = {'source': {'crates-io': {'replace-with': 'vendored-sources'},
                                   'vendored-sources': {'directory': 'vendor'}}}
            if tomllib.loads(vendor.stdout.decode()) != expected:
                raise ValueError('unexpected vendor configuration; no source archive created')
            (source / '.cargo').mkdir()
            (source / '.cargo/config.toml').write_bytes(vendor.stdout)
            report = subprocess.run([cargo_about, 'generate', '--locked', '--offline', '--fail', '--format', 'json'],
                                    cwd=source, stdout=subprocess.PIPE, stderr=log, check=True, timeout=180)
        notices = render_notices(json.loads(report.stdout), source)
        (source / 'THIRD_PARTY_NOTICES.txt').write_bytes(notices)
        (output / 'THIRD_PARTY_NOTICES.txt').write_bytes(notices)
        provenance = {'schema': 1, 'repository': 'https://github.com/davafons/parcela',
                      'revision': revision, 'version': package['version'], 'cargo_about_version': '0.9.2',
                      'source_files': {path: {'sha256': hashlib.sha256(entry.content).hexdigest(),
                                              'executable': entry.executable} for path, entry in sorted(files.items())},
                      'license_supplements': [{'crate': key[0], 'version': key[1], **value}
                                              for key, value in SUPPLEMENTS.items()]}
        (source / 'release-source.json').write_text(json.dumps(provenance, indent=2) + '\n')
        for path, entry in files.items():
            if (source / path).read_bytes() != entry.content:
                raise ValueError(f'build tool changed frozen source input: {path}')
        paths = [source / path for path in files]
        paths.extend(source / path for path in ['.cargo/config.toml', 'THIRD_PARTY_NOTICES.txt', 'release-source.json'])
        paths.extend((source / 'vendor').rglob('*'))
        archive = output / f'{name}-source.tar.gz'
        with archive.open('xb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as bundle:
                for path in sorted(paths):
                    if path.is_symlink():
                        raise ValueError(f'unsupported generated symlink: {path}')
                    if not path.is_file():
                        continue
                    info = bundle.gettarinfo(path, arcname=f'{name}/{path.relative_to(source).as_posix()}')
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    info.mtime = 0
                    info.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
                    with path.open('rb') as content:
                        bundle.addfile(info, content)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (output / f'{archive.name}.sha256').write_text(f'{digest}  {archive.name}\n')
    return archive


def verify_source(archive, revision):
    root = archive.parent / 'verification'
    root.mkdir()
    receipt = {'complete': False, 'revision': revision, 'system': platform.system(), 'machine': platform.machine()}
    report_path = root / 'receipt.json'
    report_path.write_text(json.dumps(receipt, indent=2) + '\n')
    try:
        digest, filename = (archive.parent / f'{archive.name}.sha256').read_text().split()
        if filename != archive.name or hashlib.sha256(archive.read_bytes()).hexdigest() != digest:
            raise ValueError('source archive checksum mismatch')
        name = archive.name.removesuffix('-source.tar.gz')
        extracted = root / 'extracted'
        with tarfile.open(archive) as bundle:
            for member in bundle.getmembers():
                path = PurePosixPath(member.name)
                if not member.isfile() or path.is_absolute() or '..' in path.parts or path.parts[0] != name:
                    raise ValueError(f'unsupported source archive member: {member.name}')
            bundle.extractall(extracted, filter='data')
        source = extracted / name
        provenance = json.loads((source / 'release-source.json').read_text())
        if provenance['revision'] != revision:
            raise ValueError('source revision mismatch')
        for path, entry in provenance['source_files'].items():
            check_source_path(path)
            file = source / path
            if (hashlib.sha256(file.read_bytes()).hexdigest() != entry['sha256']
                    or bool(file.stat().st_mode & 0o111) != entry['executable']):
                raise ValueError(f'extracted source mismatch: {path}')
        for directory in ['home', 'cargo-home', 'tmp', 'probe']:
            (root / directory).mkdir()
        environment = {'PATH': os.environ['PATH'], 'HOME': str(root / 'home'),
                       'CARGO_HOME': str(root / 'cargo-home'), 'CARGO_NET_OFFLINE': 'true',
                       'RUSTUP_HOME': os.environ.get('RUSTUP_HOME', str(Path.home() / '.rustup')),
                       'CARGO_TARGET_DIR': str(root / 'target'), 'TMPDIR': str(root / 'tmp')}
        command = ['cargo', 'build', '--profile', 'dist', '--frozen', '--offline']
        with (root / 'build.log').open('wb') as log:
            subprocess.run(command, cwd=source, env=environment, stdout=log, stderr=log, check=True, timeout=600)
        binary = root / 'target/dist/parcela'
        probe_environment = {'HOME': str(root / 'probe/absent-home'), 'PATH': str(root / 'probe/no-backends'),
                             'PARCELA_STATE_DIR': str(root / 'probe/absent-state')}
        outputs = {}
        for argument in ['--version', '--help', '--skill']:
            result = subprocess.run([str(binary), argument], cwd=root / 'probe', env=probe_environment,
                                    capture_output=True, check=True, timeout=15)
            if result.stderr:
                raise ValueError(f'unexpected stderr for {argument}')
            outputs[argument] = result.stdout
            (root / f'{argument.removeprefix("--")}.stdout').write_bytes(result.stdout)
        if outputs['--version'] != f"parcela {provenance['version']}\n".encode():
            raise ValueError('rebuilt version mismatch')
        if not all(word in outputs['--help'] for word in [b'Usage: parcela', b'workspace', b'proxy']):
            raise ValueError('rebuilt help is incomplete')
        if outputs['--skill'] != (source / '.agents/skills/parcela/SKILL.md').read_bytes():
            raise ValueError('rebuilt embedded skill mismatch')
        if list((root / 'probe').iterdir()):
            raise ValueError('read-only probes created application state')
        receipt.update(complete=True, command=command, archive_sha256=digest,
                       source_files_verified=len(provenance['source_files']),
                       private_empty_cargo_home=True, embedded_skill_exact=True, probes_created_no_state=True)
    except Exception as error:
        receipt['error'] = str(error)
        raise
    finally:
        report_path.write_text(json.dumps(receipt, indent=2) + '\n')
    return report_path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--cargo-about', default='cargo-about')
    parser.add_argument('--verify', action='store_true', help='rebuild the extracted archive offline with an empty Cargo home')
    args = parser.parse_args()
    cargo_about = shutil.which(args.cargo_about)
    if not cargo_about:
        parser.error('cargo-about 0.9.2 is required')
    archive = build_source(args.revision, args.output_dir.resolve(), cargo_about)
    print(archive, flush=True)
    if args.verify:
        print(verify_source(archive, args.revision))


if __name__ == '__main__':
    main()
