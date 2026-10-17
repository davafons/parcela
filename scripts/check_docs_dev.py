"""Verify the maintained docs graph in disposable jj workspaces and private backend state."""
import argparse
import hashlib
from http.client import HTTPResponse
import json
import os
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import tempfile

from check_live import BINARY, ROOT, free_port, wait_for


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    website = ROOT / 'docs/website'
    program = "import {groups, downloads} from './content.mjs'; console.log(JSON.stringify([...groups.flatMap(g => g.pages.map(p => p[2])), ...Object.values(downloads)]));"
    canonical = json.loads(subprocess.check_output(['node', '--input-type=module', '-e', program], cwd=website, text=True))
    tracked = subprocess.check_output(['jj', 'file', 'list'], cwd=ROOT, text=True).splitlines()
    inputs = sorted(set(canonical + ['parcela.yaml', '.gitignore']
                        + [name for name in tracked if name.startswith('docs/website/')]))
    hashes = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in inputs}
    root = Path(tempfile.mkdtemp(prefix='pdd-', dir='/tmp')).resolve()
    state = root / 'state'
    https_port, http_port = free_port(), free_port()
    report = {'complete': False, 'fixture': str(root), 'front_ports': [https_port, http_port],
              'backend_ports': [], 'protocol_reports': [], 'verified': []}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    copies, handles = [], []
    proxy = None

    def cli(path, *argv, check=True, timeout=45):
        return subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                              capture_output=True, text=True, check=check, timeout=timeout)

    def jj(path, *argv):
        return subprocess.check_output(['jj', *argv], cwd=path, stderr=subprocess.STDOUT, text=True)

    def request(host, path='/', expected=200):
        context = ssl.create_default_context(cafile=str(state / 'proxy/data/pki/authorities/local/root.crt'))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', https_port), timeout=5), server_hostname=host) as connection:
            connection.sendall(f'GET {path} HTTP/1.1\r\nHost: {host}:{https_port}\r\nConnection: close\r\n\r\n'.encode())
            response = HTTPResponse(connection)
            response.begin()
            body = response.read()
            assert response.status == expected, (host, path, response.status)
            return body

    def remember(result):
        session = result['session']
        assert len(session['endpoints']) == 1
        report['backend_ports'].append(session['endpoints'][0]['backend_port'])
        return session

    def protocol(path, origin, peer, name):
        output = root / f'{name}-protocol.json'
        result = subprocess.run(['node', str(website / 'check-dev.mjs'), origin, peer, str(path),
                                 str(state / 'proxy/data/pki/authorities/local/root.crt'), str(output)],
                                capture_output=True, text=True, timeout=150)
        (root / f'{name}-protocol.log').write_text(result.stdout + result.stderr)
        report['protocol_reports'].append(str(output))
        assert result.returncode == 0, f'Native HMR check failed; inspect {output}'
        assert json.loads(output.read_text())['complete']

    try:
        primary, preview = root / 'primary', root / 'preview'
        primary.mkdir()
        for name in inputs:
            destination = primary / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)
        with (primary / '.gitignore').open('a') as ignore:
            ignore.write('\n/.docs-dev-fixture.json\n')
        (root / 'jj-init.log').write_text(jj(primary, 'git', 'init'))
        (root / 'jj-workspace.log').write_text(jj(primary, 'workspace', 'add', '--name', 'review', '-r', '@', str(preview)))
        for path in [primary, preview]:
            copies.append(path)
            (path / 'docs/website/node_modules').symlink_to(website / 'node_modules', target_is_directory=True)
            (path / '.docs-dev-fixture.json').write_text(json.dumps({'source': str(path)}) + '\n')
            assert json.loads(cli(path, 'config', 'check').stdout)['valid']
        operation = jj(primary, '--ignore-working-copy', 'op', 'log', '--limit', '1', '--no-graph', '-T', 'id')
        discovery = json.loads(cli(primary, 'workspace', 'discover').stdout)
        assert {row['source'] for row in discovery['checkouts']} == {str(primary), str(preview)}
        assert all(row['vcs'] == 'jj' and row['manifest_exists'] for row in discovery['checkouts'])
        assert not discovery['automatic_adoption'] and not state.exists()
        assert operation == jj(primary, '--ignore-working-copy', 'op', 'log', '--limit', '1', '--no-graph', '-T', 'id')
        report['verified'].append('Real sibling jj discovery is read-only and finds the maintained root manifest')
        for path, role, label in [(primary, 'primary', 'primary'), (preview, 'preview', 'review')]:
            adopted = json.loads(cli(path, 'workspace', 'adopt', '--role', role, '--label', label).stdout)
            assert adopted['workspace']['role'] == role
        log = open(root / 'proxy.log', 'w')
        handles.append(log)
        proxy = subprocess.Popen([str(BINARY), '--state-dir', str(state), 'proxy', 'run', '--caddy', args.caddy,
                                  '--test-profile', '--http-port', str(http_port), '--https-port', str(https_port)],
                                 stdout=log, stderr=log)
        wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
        sessions, origins = [], []
        for path, host in [(primary, 'parcela-docs.localhost'), (preview, 'review-parcela-docs.localhost')]:
            started = cli(path, 'up', '--detach', '--process-compose', args.process_compose,
                          '--backend-port', str(free_port()), '--readiness-timeout', '60', timeout=90)
            session = remember(json.loads(started.stdout))
            sessions.append(session)
            status = json.loads(cli(path, 'status').stdout)
            assert status['state'] == 'ready' and status['live_health_verified']
            assert status['live']['endpoints']['docs']['listener_owned']
            assert cli(path, 'url').stdout.strip() == f'https://{host}:{https_port}'
            origins.append(cli(path, 'url').stdout.strip())
            assert b'<div id="app"></div>' in request(host), 'Native VitePress app bootstrap missing'
            assert b'Run development services' in request(host, '/index.md')
            services = json.loads(cli(path, 'services').stdout)['services']
            assert {entry['name']: entry['status'] for entry in services} == {'docs': 'Running'}
            assert 'vitepress' in cli(path, 'logs', 'docs', '--tail', '50').stdout.lower()
        report['verified'].append('Two owned loopback VitePress graphs with distinct HTTPS URLs, ports, logs and generated/cache paths')
        environment = {**os.environ, 'DOCS_BASE': '/', 'DOCS_SITE': origins[0]}
        collision = subprocess.run(['npm', 'run', 'dev', '--prefix', str(primary / 'docs/website'), '--',
                                    '--port', str(sessions[0]['backend_port'])],
                                   env=environment, capture_output=True, text=True, timeout=30)
        (root / 'strict-port.log').write_text(collision.stdout + collision.stderr)
        assert collision.returncode != 0 and 'already in use' in collision.stdout + collision.stderr
        assert json.loads(cli(primary, 'status').stdout)['state'] == 'ready'
        report['verified'].append('Occupied-port startup refuses without fallback or takeover')
        protocol(primary, origins[0], origins[1], 'primary')
        built = cli(primary, 'task', 'docs-build', timeout=150)
        (root / 'build-task.log').write_text(built.stdout + built.stderr)
        assert f'href="{origins[0]}/"' in (primary / 'target/docs/index.html').read_text()
        assert json.loads(cli(primary, 'status').stdout)['state'] == 'ready'
        report['verified'].append('Explicit finite build task produces the environment canonical URL without stopping dev')
        index_source = primary / 'docs/site/index.md'
        original_index = index_source.read_bytes()
        index_source.write_bytes(original_index + b'\n\n## RestartSearchProof73942\n\nFresh development search after restart.\n')
        wait_for(lambda: b'RestartSearchProof73942' in request('parcela-docs.localhost', '/index.md'), [proxy])
        report['dev_search_refreshed_before_restart'] = b'RestartSearchProof73942' in request('parcela-docs.localhost', '/@localSearchIndexroot')
        restarted = remember(json.loads(cli(primary, 'restart', '--detach', '--process-compose', args.process_compose,
                                              '--backend-port', str(free_port()), timeout=90).stdout))
        assert restarted['generation'] == sessions[0]['generation'] + 1 and restarted['token'] != sessions[0]['token']
        assert b'Run development services' in request('parcela-docs.localhost', '/index.md')
        assert b'RestartSearchProof73942' in request('parcela-docs.localhost', '/@localSearchIndexroot')
        index_source.write_bytes(original_index)
        wait_for(lambda: b'RestartSearchProof73942' not in request('parcela-docs.localhost', '/index.md'), [proxy])
        report['verified'].append('Explicit restart rebuilds development search after canonical heading changes')
        protocol(preview, origins[1], origins[0], 'preview')
        report['verified'].append('Restart creates a new generation/nonce while the independent peer retains live HMR')
        cli(primary, 'down')
        request('parcela-docs.localhost', expected=503)
        assert b'<div id="app"></div>' in request('review-parcela-docs.localhost')
        report['verified'].append('Independent stop withdraws one route without stopping its peer')
        report['complete'] = True
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        cleanup_errors = []
        for path in copies:
            try:
                stopped = cli(path, 'down', check=False)
                if stopped.returncode != 0:
                    cleanup_errors.append(f'Stop refused for {path}; see retained fixture')
            except Exception as error:
                cleanup_errors.append(str(error))
        if proxy is not None and proxy.poll() is None:
            proxy.terminate()
            try:
                proxy.wait(timeout=15)
            except subprocess.TimeoutExpired:
                cleanup_errors.append('Owned proxy did not exit before cleanup deadline')
        for handle in handles:
            handle.close()
        for context_file in (state / 'environments').glob('*/context.json'):
            context = json.loads(context_file.read_text())
            report['backend_ports'].extend(endpoint['backend_port'] for endpoint in context['session']['endpoints'])
        report['backend_ports'] = sorted(set(report['backend_ports']))
        report['source_unchanged'] = all(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest for name, digest in hashes.items())
        report['listeners_closed'] = True
        for port in [https_port, http_port, *report['backend_ports']]:
            with socket.socket() as connection:
                if connection.connect_ex(('127.0.0.1', port)) == 0:
                    report['listeners_closed'] = False
        report['cleanup_errors'] = cleanup_errors
        report['complete'] = report['complete'] and report['source_unchanged'] and report['listeners_closed'] and not cleanup_errors
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    assert report['complete'], report
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
