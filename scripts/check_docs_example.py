"""Drive the downloadable tutorial demo through real isolated Parcela backends."""
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

INPUTS = ['parcela.yaml', 'frontend/index.html', 'backend/status.json']


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
    https, http = free_port(), free_port()
    report = {'complete': False, 'fixture': str(root), 'front_ports': [https, http], 'backend_ports': []}
    handles, children, copies = [], [], []

    def cli(path, *argv, check=True):
        return subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                              capture_output=True, text=True, check=check, timeout=30)

    def request(host, path, expected=200):
        context = ssl.create_default_context(cafile=str(state / 'proxy/data/pki/authorities/local/root.crt'))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', https), timeout=3), server_hostname=host) as connection:
            connection.sendall(f'GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n'.encode())
            response = b''
            while part := connection.recv(16384):
                response += part
        head, body = response.split(b'\r\n\r\n', 1)
        if not head.startswith(f'HTTP/1.1 {expected}'.encode()):
            raise ValueError(f'expected {expected}, got {head!r}')
        return body

    def remember(result):
        session = result['session']
        assert len(session['endpoints']) == 2
        report['backend_ports'].extend(endpoint['backend_port'] for endpoint in session['endpoints'])
        return session

    try:
        for label, role in [('primary', 'primary'), ('review', 'preview')]:
            path = root / label
            for name in INPUTS:
                destination = path / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source / name, destination)
            copies.append(path)
            checked = json.loads(cli(path, 'config', 'check').stdout)
            assert checked['valid'] is True
        log = open(root / 'proxy.log', 'w')
        handles.append(log)
        proxy = subprocess.Popen([str(BINARY), '--state-dir', str(state), 'proxy', 'run', '--caddy', args.caddy,
                                  '--test-profile', '--http-port', str(http), '--https-port', str(https)], stdout=log, stderr=log)
        children.append(proxy)
        wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
        sessions = []
        for index, (path, base) in enumerate(zip(copies, ['parcela-demo.localhost', 'review-parcela-demo.localhost'])):
            naming = [] if index == 0 else ['--name', 'review']
            started = json.loads(cli(path, 'up', *naming, '--detach', '--process-compose', args.process_compose).stdout)
            context = json.loads(cli(path, 'context').stdout)
            assert context['workspace']['role'] == ('primary' if index == 0 else 'preview')
            assert set(context['planned_urls']) == {'frontend', 'backend'}
            session = remember(started)
            sessions.append(session)
            assert session['generation'] == 2
            status = json.loads(cli(path, 'status').stdout)
            assert status['state'] == 'ready' and status['live_health_verified']
            assert all(endpoint['listener_owned'] and endpoint['http_ready'] for endpoint in status['live']['endpoints'].values())
            assert cli(path, 'url').stdout.strip() == f'https://{base}:{https}'
            assert cli(path, 'url', '--service', 'backend').stdout.strip() == f'https://api-{base}:{https}'
            assert b'Parcela demo frontend' in request(base, '/')
            assert json.loads(request(f'api-{base}', '/status.json')) == {'service': 'backend', 'ready': True}
            services = json.loads(cli(path, 'services').stdout)['services']
            assert {entry['name']: entry['status'] for entry in services} == {'frontend': 'Running', 'backend': 'Running', 'worker': 'Running'}
            wait_for(lambda: 'demo worker ready' in cli(path, 'logs', 'worker', '--tail', '10').stdout, [proxy])
        primary = copies[0]
        restarted = remember(json.loads(cli(primary, 'restart', '--detach', '--process-compose', args.process_compose).stdout))
        assert restarted['generation'] == 3 and restarted['token'] != sessions[0]['token']
        assert json.loads(request('api-parcela-demo.localhost', '/status.json')) == {'service': 'backend', 'ready': True}
        cli(primary, 'down')
        assert json.loads(cli(primary, 'status').stdout)['state'] == 'stopped'
        request('parcela-demo.localhost', '/', 503)
        request('api-parcela-demo.localhost', '/status.json', 503)
        assert json.loads(request('api-review-parcela-demo.localhost', '/status.json'))['ready'] is True
        for path in copies:
            for name, digest in hashes.items():
                assert hashlib.sha256((path / name).read_bytes()).hexdigest() == digest
        report['complete'] = True
        report['verified'] = ['organizer validation', 'automatic primary/named-preview registration', 'planned context', 'two named TLS origins per environment',
                              'owned readiness', 'native frontend/backend/worker', 'worker logs', 'whole-graph generation restart',
                              'independent stop and route withdrawal', 'source retention']
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        for context_file in (state / 'environments').glob('*/context.json'):
            context = json.loads(context_file.read_text())
            report['backend_ports'].extend(endpoint['backend_port'] for endpoint in context['session']['endpoints'])
        report['backend_ports'] = sorted(set(report['backend_ports']))
        for path in copies:
            stopped = cli(path, 'down', check=False)
            if stopped.returncode != 0:
                report['complete'] = False
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
        for handle in handles:
            handle.close()
        report['source_unchanged'] = all(hashlib.sha256((source / name).read_bytes()).hexdigest() == digest for name, digest in hashes.items())
        report['listeners_closed'] = True
        for port in [https, http, *report['backend_ports']]:
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
