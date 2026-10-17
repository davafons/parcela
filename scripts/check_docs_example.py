"""Drive the getting-started app through real isolated Parcela backends: two checkouts, then cleanup."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import tempfile

from check_live import BINARY, ROOT, free_port, wait_for

INPUTS = ['parcela.yaml', 'index.html', 'api.py', 'worker.py']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = ROOT / 'examples/docs-demo'
    hashes = {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in INPUTS}
    root = Path(tempfile.mkdtemp(prefix='pdemo-', dir='/tmp')).resolve()
    state = root / 'state'
    https = free_port()
    report = {'complete': False, 'fixture': str(root), 'front_ports': [https], 'backend_ports': []}
    handles, children = [], []
    primary, signup = root / 'hello', root / 'signup'

    def cli(path, *argv, check=True):
        return subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                              capture_output=True, text=True, check=check, timeout=30)

    def git(*argv):
        subprocess.run(['git', '-C', str(primary), '-c', 'user.name=docs', '-c', 'user.email=docs@example.invalid', *argv],
                       check=True, capture_output=True, timeout=30)

    def request(host, path='/', expected=200):
        context = ssl.create_default_context(cafile=str(state / 'proxy/data/pki/authorities/local/root.crt'))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', https), timeout=3), server_hostname=host) as connection:
            connection.sendall(f'GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n'.encode())
            response = b''
            while part := connection.recv(16384):
                response += part
        head, body = response.split(b'\r\n\r\n', 1)
        if not head.startswith(f'HTTP/1.1 {expected}'.encode()):
            raise ValueError(f'expected {expected}, got {head!r}')
        if b'transfer-encoding: chunked' not in head.lower():
            return body
        decoded = b''
        while True:
            size, _, body = body.partition(b'\r\n')
            if not int(size, 16):
                return decoded
            decoded, body = decoded + body[:int(size, 16)], body[int(size, 16) + 2:]

    def remember(result):
        session = result['session']
        assert {endpoint['service'] for endpoint in session['endpoints']} == {'web', 'api'}
        report['backend_ports'].extend(endpoint['backend_port'] for endpoint in session['endpoints'])
        return session

    try:
        primary.mkdir()
        for name in INPUTS:
            shutil.copyfile(source / name, primary / name)
        git('init', '-q')
        git('add', '.')
        git('commit', '-qm', 'hello app')
        git('worktree', 'add', '-q', str(signup))
        log = open(root / 'proxy.log', 'w')
        handles.append(log)
        proxy = subprocess.Popen([str(BINARY), '--state-dir', str(state), 'proxy', 'run', '--caddy', args.caddy,
                                  '--port', str(https)], stdout=log, stderr=log)
        children.append(proxy)
        wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
        sessions, database_urls = [], []
        for path, base in [(primary, 'hello.localhost'), (signup, 'signup-hello.localhost')]:
            sessions.append(remember(json.loads(cli(path, 'up', '--detach', '--process-compose', args.process_compose).stdout)))
            status = json.loads(cli(path, 'status').stdout)
            assert status['state'] == 'ready' and status['live_health_verified']
            assert all(endpoint['listener_owned'] and endpoint['http_ready'] for endpoint in status['live']['endpoints'].values())
            assert cli(path, 'url').stdout.strip() == f'https://{base}:{https}'
            assert cli(path, 'url', '--service', 'api').stdout.strip() == f'https://api-{base}:{https}'
            assert b'Hello from Parcela' in request(base)
            assert json.loads(request(f'api-{base}')) == {'message': 'hello from the API', 'url': f'https://api-{base}:{https}'}
            processes = json.loads(cli(path, 'status').stdout)['processes']
            assert {entry['name']: entry['state'] for entry in processes} == {'web': 'running', 'api': 'running', 'ticker': 'running'}, processes
            wait_for(lambda: 'tick 1' in cli(path, 'logs', 'ticker', '--tail', '10').stdout, [proxy])
            database_url = cli(path, 'task', 'db-url').stdout.strip()
            assert database_url.startswith('postgres://127.0.0.1:') and database_url.endswith('/hello')
            database_urls.append(database_url)
        assert len(set(database_urls)) == 2, database_urls
        listed = json.loads(cli(primary, 'ls').stdout)['workspaces']
        assert {Path(entry['workspace']['source']).resolve() for entry in listed} >= {primary, signup}
        restarted = remember(json.loads(cli(primary, 'restart', '--detach', '--process-compose', args.process_compose).stdout))
        assert restarted['generation'] == sessions[0]['generation'] + 1 and restarted['token'] != sessions[0]['token']
        assert cli(primary, 'task', 'db-url').stdout.strip() == database_urls[0]
        cli(primary, 'down')
        assert json.loads(cli(primary, 'status').stdout)['state'] == 'stopped'
        request('hello.localhost', '/', 503)
        assert b'Hello from Parcela' in request('signup-hello.localhost')
        cli(primary, 'down', '--all')
        git('worktree', 'remove', str(signup))
        cli(primary, 'rm', '--prune')
        remaining = {Path(entry['workspace']['source']).resolve() for entry in json.loads(cli(primary, 'ls').stdout)['workspaces']}
        assert signup not in remaining and primary in remaining, remaining
        for name, digest in hashes.items():
            assert hashlib.sha256((primary / name).read_bytes()).hexdigest() == digest
        report['complete'] = True
        report['verified'] = ['primary and Git worktree environments', 'named HTTPS web and API per checkout',
                              'per-service URL variables', 'worker logs', 'distinct named ports per checkout',
                              'generation restart keeps the named port', 'independent stop and route withdrawal',
                              'down --all, worktree removal and rm --prune', 'source retention']
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        for context_file in (state / 'environments').glob('*/context.json'):
            context = json.loads(context_file.read_text())
            report['backend_ports'].extend(endpoint['backend_port'] for endpoint in context['session']['endpoints'])
        report['backend_ports'] = sorted(set(report['backend_ports']))
        for path in [primary, signup]:
            if path.exists() and cli(path, 'down', check=False).returncode != 0:
                report['complete'] = False
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
        for handle in handles:
            handle.close()
        report['source_unchanged'] = all(hashlib.sha256((source / name).read_bytes()).hexdigest() == digest for name, digest in hashes.items())
        report['listeners_closed'] = True
        for port in [https, *report['backend_ports']]:
            with socket.socket() as connection:
                if connection.connect_ex(('127.0.0.1', port)) == 0:
                    report['listeners_closed'] = False
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        assert report['source_unchanged'] and report['listeners_closed'], report
    assert report['complete'], report
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
