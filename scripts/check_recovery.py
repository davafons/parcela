import argparse
import json
import os
from pathlib import Path
import signal
import socket
import sqlite3
import ssl
import subprocess
import tempfile
import threading
import time
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.client import HTTPResponse

SERVICE = '''import json, os, signal, time
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
root = Path(os.environ['PARCELA_RUNTIME_DIR'])
def stop(*_):
    (root / 'shutdown-started').write_text('started')
    time.sleep(float(os.environ.get('STOP_DELAY', '0')))
    (root / 'shutdown-completed').write_text('completed')
    raise SystemExit(0)
signal.signal(signal.SIGTERM, stop)
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/stream':
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            for tick in range(12):
                self.wfile.write(f'data: {tick}\\n\\n'.encode())
                self.wfile.flush()
                time.sleep(1)
            return
        data = json.dumps({'id': os.environ['PARCELA_ENVIRONMENT_ID'], 'pid': os.getpid(), 'generation': os.environ['PARCELA_GENERATION']}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *_): pass
ThreadingHTTPServer(('127.0.0.1', int(os.environ['PORT'])), Handler).serve_forever()
'''

def free_port(exclude=()):
    while True:
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        if port not in exclude:
            return port

def wait_for(check, timeout=15):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, ValueError, subprocess.CalledProcessError) as failure:
            last = repr(failure)
        time.sleep(.05)
    raise RuntimeError(f'fixture wait timed out: {last}')

def listener_pids(port):
    binary = '/usr/sbin/lsof' if Path('/usr/sbin/lsof').exists() else 'lsof'
    result = subprocess.run([binary, '-nP', f'-iTCP:{port}', '-sTCP:LISTEN', '-Fp'], capture_output=True, text=True, timeout=5)
    if result.returncode == 1 and not result.stdout:
        return []
    assert result.returncode == 0, result.stderr
    return [int(line[1:]) for line in result.stdout.splitlines() if line.startswith('p')]

def listening(port):
    return bool(listener_pids(port))

class Fixture:
    def __init__(self, args, name):
        self.args = args
        self.root = Path(tempfile.mkdtemp(prefix='pcli-r-', dir='/tmp')).resolve()
        self.state = self.root / 's'
        self.apps = []
        self.owners = []
        self.logs = []
        self.https = free_port()
        self.http = free_port([self.https])
        self.proxy = None
        self.name = name
    def cli(self, path, *args, timeout=25):
        return subprocess.run([self.args.binary, '--path', str(path), '--state-dir', str(self.state), *args], capture_output=True, text=True, timeout=timeout)
    def spawn(self, path, *args):
        log = open(self.root / f'child-{len(self.owners)}.log', 'w')
        self.logs.append(log)
        child = subprocess.Popen([self.args.binary, '--path', str(path), '--state-dir', str(self.state), *args], stdout=log, stderr=log)
        self.owners.append(child)
        return child
    def app(self, label, delay=0):
        path = self.root / label
        path.mkdir()
        (path / 'service.py').write_text(SERVICE)
        manifest = {'version': 1, 'project': {'id': 'reliability-review', 'name': 'review'}, 'default_service': 'web',
                    'services': {'web': {'readiness_path': '/ready'}}, 'inherit_environment': False,
                    'environment': {'STOP_DELAY': str(delay)},
                    'process_compose': {'version': '0.5', 'processes': {'web': {'entrypoint': ['python3', str(path / 'service.py')],
                       'availability': {'restart': 'no'}, 'shutdown': {'signal': 15, 'timeout_seconds': 15}}}}}
        (path / 'parcela.yaml').write_text(json.dumps(manifest))
        adopted = self.cli(path, 'workspace', 'adopt', '--role', 'preview', '--label', label)
        assert adopted.returncode == 0, adopted.stderr
        identity = json.loads(adopted.stdout)['workspace']['id']
        app = {'path': path, 'id': identity, 'host': f'{label}-review.localhost', 'runtime': self.state / 'environments' / identity,
               'port': free_port([self.https, self.http]), 'manifest': manifest}
        self.apps.append(app)
        return app
    def start_proxy(self):
        self.proxy = self.spawn(self.root, 'proxy', 'run', '--caddy', self.args.caddy, '--test-profile', '--https-port', str(self.https), '--http-port', str(self.http))
        wait_for(lambda: self.cli(self.root, 'proxy', 'status').returncode == 0)
    def start(self, app):
        owner = self.spawn(app['path'], 'up', '--process-compose', self.args.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
        app['owner'] = owner
        wait_for(lambda: self.request(app)[0] == 200)
        return owner
    def session(self, app):
        with sqlite3.connect(f'file:{self.state / "registry.sqlite3"}?mode=ro', uri=True) as db:
            db.row_factory = sqlite3.Row
            return dict(db.execute('SELECT * FROM sessions WHERE environment_id=?', (app['id'],)).fetchone())
    def request(self, app):
        context = ssl.create_default_context(cafile=str(self.state / 'proxy/data/pki/authorities/local/root.crt'))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', self.https), timeout=2), server_hostname=app['host']) as stream:
            stream.settimeout(2)
            stream.sendall(f'GET /ready HTTP/1.1\r\nHost: {app["host"]}\r\nConnection: close\r\n\r\n'.encode())
            data = b''
            while True:
                chunk = stream.recv(65536)
                if not chunk: break
                data += chunk
        head, body = data.split(b'\r\n\r\n', 1)
        return int(head.split(b' ', 2)[1]), body.decode()
    def caddy_config(self):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(3)
            stream.connect(str(self.state / 'proxy/admin.sock'))
            stream.sendall(b'GET /config/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n')
            with HTTPResponse(stream) as response:
                response.begin()
                assert response.status == 200
                return json.loads(response.read())
    def pc_down(self, app):
        context = json.loads((app['runtime'] / 'context.json').read_text())
        assert context['session']['environment_id'] == app['id']
        token = ['--token-file', str(app['runtime'] / 'pc.token')] if (app['runtime'] / 'pc.token').exists() else []
        return subprocess.run([self.args.compose, *token, '--use-uds', '--unix-socket', str(app['runtime'] / 'pc.sock'), '--log-file', str(self.root / 'native-client.log'), 'down'],
                              capture_output=True, text=True, timeout=20)
    def close(self):
        for app in self.apps:
            if (app['runtime'] / 'pc.sock').exists():
                try:
                    self.cli(app['path'], 'down')
                    if (app['runtime'] / 'pc.sock').exists(): self.pc_down(app)
                except (OSError, subprocess.TimeoutExpired): pass
        for child in reversed(self.owners):
            if child.poll() is None:
                child.terminate()
                try: child.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
        for app in self.apps:
            wait_for(lambda: not listening(app['port']), timeout=12)
        for log in self.logs: log.close()
    def details(self):
        return {'fixture': str(self.root), 'https_port': self.https, 'http_port': self.http}

def owner_crash(f):
    app = f.app('crash')
    f.start_proxy()
    owner = f.start(app)
    owner.kill()
    assert owner.wait(timeout=5) == -signal.SIGKILL
    status = json.loads(f.cli(app['path'], 'status').stdout)
    assert status['state'] == 'unknown'
    wait_for(lambda: f.request(app)[0] == 503, timeout=10)
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0, down.stderr
    assert not listening(app['port']), 'recovered stop must finish the actual graph'
    class Foreign(BaseHTTPRequestHandler):
        def do_GET(self):
            data = b'FOREIGN-BACKEND'
            self.send_response(200)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        def log_message(self, *_): pass
    server = ThreadingHTTPServer(('127.0.0.1', app['port']), Foreign)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        code, body = f.request(app)
        assert code == 503 and 'FOREIGN-BACKEND' not in body, 'stale hostname must not serve an unowned reused port'
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)
    return {'owner_crash_route': 503, 'recovered_down_exit': down.returncode, 'foreign_reused_port_route': code}

def graceful_stop(f):
    app = f.app('grace', delay=7)
    f.start_proxy()
    owner = f.start(app)
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0, down.stderr
    assert f.session(app)['status'] == 'stopped'
    assert (app['runtime'] / 'shutdown-completed').exists(), 'stop success requires completed native shutdown'
    assert not listener_pids(app['port']), 'stop success must not leave a supervised listener'
    assert not (app['runtime'] / 'pc.sock').exists(), 'normal shutdown must remove its native socket'
    assert owner.wait(timeout=5) == 0
    retry = f.cli(app['path'], 'up', '--detach', '--process-compose', f.args.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
    assert retry.returncode == 0, retry.stderr
    return {'down_exit': down.returncode, 'shutdown_completed': True, 'subsequent_up_exit': retry.returncode}

def duplicate_stop(f):
    app = f.app('duplicate', delay=3)
    f.start_proxy()
    owner = f.start(app)
    command = [f.args.binary, '--path', str(app['path']), '--state-dir', str(f.state), 'down']
    first = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    wait_for(lambda: (app['runtime'] / 'shutdown-started').exists())
    second = f.cli(app['path'], 'down')
    out, err = first.communicate(timeout=15)
    assert first.returncode == 0, err
    assert second.returncode == 0, second.stderr
    assert f.session(app)['status'] == 'stopped'
    assert owner.wait(timeout=5) == 0
    return {'first_down_exit': first.returncode, 'duplicate_down_exit': second.returncode, 'final_state': 'stopped'}

def restart_preflight(f):
    app = f.app('preflight')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    restart = f.cli(app['path'], 'restart', '--detach', '--process-compose', '/nonexistent/parcela-review-compose')
    assert restart.returncode != 0
    assert owner.poll() is None, 'failed replacement preflight must preserve the original owner'
    assert f.session(app)['token'] == original['token']
    assert f.request(app)[0] == 200, 'failed preflight must preserve the healthy route'
    return {'restart_exit': restart.returncode, 'original_owner_alive': True, 'https_code': 200}

def proxy_outage(f):
    apps = [f.app('first'), f.app('second')]
    f.start_proxy()
    owners = [f.start(app) for app in apps]
    original = [f.session(app) for app in apps]
    f.proxy.terminate()
    f.proxy.wait(timeout=10)
    time.sleep(1)
    assert all(owner.poll() is None for owner in owners), 'front-door loss must not stop unrelated application graphs'
    assert all(listening(app['port']) for app in apps)
    f.start_proxy()
    for app, old in zip(apps, original):
        wait_for(lambda: f.request(app)[0] == 200)
        assert f.session(app)['token'] == old['token']
        assert f.session(app)['generation'] == old['generation']
    return {'application_owners_preserved': True, 'routes_recovered': 2, 'application_generations_preserved': True}

def lost_stop_reply(f):
    app = f.app('lost', delay=2)
    f.start_proxy()
    owner = f.start(app)
    session = f.session(app)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(app['runtime'] / 'ctl.sock'))
        client.sendall((json.dumps({'environment_id': app['id'], 'token': session['token'], 'generation': session['generation'], 'action': {'type': 'stop'}}) + '\n').encode())
    wait_for(lambda: f.session(app)['status'] in ['stopping', 'stopped'])
    retry = f.cli(app['path'], 'down')
    assert retry.returncode == 0, retry.stderr
    assert owner.wait(timeout=5) == 0
    return {'lost_reply_retry_exit': 0, 'shutdown_completed': (app['runtime'] / 'shutdown-completed').exists()}

def recover_surviving_graph(f):
    app = f.app('reattach')
    f.start_proxy()
    owner = f.start(app)
    before = f.session(app)
    service = json.loads(f.request(app)[1])
    owner.kill()
    owner.wait(timeout=5)
    recovered = f.cli(app['path'], 'recover', '--detach')
    assert recovered.returncode == 0, recovered.stderr
    after = f.session(app)
    assert after['token'] == before['token'] and after['generation'] == before['generation']
    assert json.loads(f.request(app)[1])['pid'] == service['pid'], 'recovery must not replay or replace service processes'
    return {'controller_recovered': True, 'service_pid_preserved': True, 'generation_preserved': True}

def guardian_death(f):
    app = f.app('guardian')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    f.proxy.kill()
    f.proxy.wait(timeout=5)
    wait_for(lambda: f.request(app)[0] == 503, timeout=10)
    assert owner.poll() is None and listening(app['port'])
    status = json.loads(f.cli(app['path'], 'status').stdout)
    assert status['state'] == 'unhealthy' and not status['live']['route_published']
    f.start_proxy()
    wait_for(lambda: f.request(app)[0] == 200)
    assert f.session(app)['token'] == original['token']
    return {'lease_expired_without_guardian': True, 'existing_caddy_recovered': True, 'application_preserved': True}

def wrong_native_token(f):
    app = f.app('auth')
    f.start_proxy()
    owner = f.start(app)
    owner.kill()
    owner.wait(timeout=5)
    token = app['runtime'] / 'pc.token'
    original = token.read_text()
    try:
        token.write_text('wrong-test-token')
        down = f.cli(app['path'], 'down')
        assert down.returncode != 0 and listening(app['port']), 'failed authentication must not stop or adopt the graph'
    finally:
        token.write_text(original)
    return {'unauthenticated_recovery_refused': True, 'service_preserved': True}

def interrupted_stop(f):
    app = f.app('interrupted', delay=3)
    f.start_proxy()
    owner = f.start(app)
    session = f.session(app)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(app['runtime'] / 'ctl.sock'))
        client.sendall((json.dumps({'environment_id': app['id'], 'token': session['token'], 'generation': session['generation'], 'action': {'type': 'stop'}}) + '\n').encode())
    wait_for(lambda: (app['runtime'] / 'shutdown-started').exists())
    owner.kill()
    owner.wait(timeout=5)
    wait_for(lambda: (app['runtime'] / 'shutdown-completed').exists())
    wait_for(lambda: not listening(app['port']))
    retry = f.cli(app['path'], 'down')
    assert retry.returncode == 0, retry.stderr
    assert f.session(app)['status'] == 'stopped'
    assert f.request(app)[0] == 503
    return {'interrupted_down_retry_exit': 0, 'shutdown_completed': True, 'route_withdrawn': True}


def completed_stop_receipt(f):
    app = f.app('receipt')
    f.start_proxy()
    owner = f.start(app)
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0, down.stderr
    assert owner.wait(timeout=5) == 0
    record = json.loads((app['runtime'] / 'owner.json').read_text())
    assert record['phase'] == 'stopped'
    with sqlite3.connect(f.state / 'registry.sqlite3') as db:
        db.execute('UPDATE sessions SET status=? WHERE environment_id=?', ('stopping', app['id']))
    retry = f.cli(app['path'], 'down')
    assert retry.returncode == 0, retry.stderr
    assert f.session(app)['status'] == 'stopped'
    assert not listening(app['port'])
    return {'completed_receipt_reconciled': True, 'retry_exit': 0}


def replaced_native_socket(f):
    app = f.app('socket')
    f.start_proxy()
    owner = f.start(app)
    owner.kill()
    owner.wait(timeout=5)
    path = app['runtime'] / 'pc.sock'
    retained = app['runtime'] / 'retained.sock'
    path.rename(retained)
    try:
        with socket.socket(socket.AF_UNIX) as foreign:
            foreign.bind(str(path))
            foreign.listen()
            inode = path.stat().st_ino
            down = f.cli(app['path'], 'down')
            assert down.returncode != 0, 'replaced native socket must not authorize shutdown'
            assert path.stat().st_ino == inode, 'foreign socket must be preserved'
            assert listening(app['port']), 'original graph must not be signaled'
            assert (app['runtime'] / 'ctl.sock').exists(), 'refusal must preserve original control evidence'
            wait_for(lambda: f.request(app)[0] == 503)
    finally:
        path.unlink()
        retained.rename(path)
    return {'replacement_refused': True, 'foreign_socket_preserved': True, 'original_graph_preserved': True}


def replaced_process_birth(f):
    app = f.app('birth')
    f.start_proxy()
    owner = f.start(app)
    owner.kill()
    owner.wait(timeout=5)
    path = app['runtime'] / 'owner.json'
    original = path.read_bytes()
    record = json.loads(original)
    record['supervisor']['birth'] += 1
    try:
        path.write_text(json.dumps(record))
        down = f.cli(app['path'], 'down')
        assert down.returncode != 0, 'mismatched birth must not authorize native shutdown'
        assert listening(app['port']), 'mismatched process identity must not be signaled'
        wait_for(lambda: f.request(app)[0] == 503)
    finally:
        path.write_bytes(original)
    return {'mismatched_birth_refused': True, 'graph_preserved': True}


def long_stream_lease(f):
    app = f.app('stream')
    f.start_proxy()
    owner = f.start(app)
    loaded = f.caddy_config()
    context = ssl.create_default_context(cafile=str(f.state / 'proxy/data/pki/authorities/local/root.crt'))
    with context.wrap_socket(socket.create_connection(('127.0.0.1', f.https), timeout=3), server_hostname=app['host']) as stream:
        stream.settimeout(3)
        stream.sendall(f'GET /stream HTTP/1.1\r\nHost: {app["host"]}\r\nConnection: close\r\n\r\n'.encode())
        with stream.makefile('rb') as response:
            assert response.readline().startswith(b'HTTP/1.1 200')
            while True:
                header = response.readline()
                assert header, 'stream closed before response headers'
                if header == b'\r\n':
                    break
            for tick in range(12):
                line = response.readline()
                while not line.startswith(b'data: '):
                    assert line, f'stream closed before event {tick}'
                    line = response.readline()
                assert line.strip() == f'data: {tick}'.encode()
                assert f.request(app)[0] == 200, 'lease renewal must keep new requests routed'
    assert owner.poll() is None
    assert f.caddy_config() == loaded, 'lease renewal must not change actual Caddy config'
    return {'stream_events': 12, 'lease_windows_crossed': True, 'config_unchanged': True}


CASES = [owner_crash, graceful_stop, duplicate_stop, restart_preflight, proxy_outage, lost_stop_reply,
         recover_surviving_graph, guardian_death, wrong_native_token, interrupted_stop, completed_stop_receipt,
         replaced_native_socket, replaced_process_birth, long_stream_lease]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True)
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--compose', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--case', action='append', choices=[case.__name__ for case in CASES])
    args = parser.parse_args()
    os.umask(0o077)
    report = {'mode': 'recovery-acceptance', 'cases': []}
    for case in CASES:
        if args.case and case.__name__ not in args.case:
            continue
        fixture = Fixture(args, case.__name__)
        record = {'case': case.__name__, **fixture.details()}
        try:
            record['evidence'] = case(fixture)
            record['confirmed'] = True
        except Exception:
            record['confirmed'] = False
            record['error'] = traceback.format_exc()
        finally:
            try:
                fixture.close()
                record['test_services_closed'] = True
            except Exception:
                record['cleanup_error'] = traceback.format_exc()
            report['cases'].append(record)
            Path(args.output).write_text(json.dumps(report, indent=2))
            print(json.dumps(record), flush=True)
    raise SystemExit(0 if all(case.get('confirmed') and case.get('test_services_closed') for case in report['cases']) else 1)

if __name__ == '__main__': main()
