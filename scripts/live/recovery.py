"""Owner, proxy and Process Compose crash, stop and recovery races. Each case runs in its own fixture."""
import json
import os
from pathlib import Path
import signal
import shutil
import socket
import sqlite3
import ssl
import subprocess
import threading
import time
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from http.client import HTTPResponse

from live.common import BINARY, Live, free_port, wait_for

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
        gated = self.path == '/ready' and os.environ.get('READINESS_GATED') == 'yes' and not (root / 'allow-ready').exists()
        self.send_response(503 if gated else 200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *_): pass
server = ThreadingHTTPServer(('127.0.0.1', int(os.environ['PORT'])), Handler)
(root / 'service-started').write_text(str(os.getpid()))
server.serve_forever()
'''

def socket_path(app, kind):
    return Path(json.loads((app['runtime'] / 'context.json').read_text())['sockets'][kind])

def socket_live(path):
    with socket.socket(socket.AF_UNIX) as probe:
        return probe.connect_ex(str(path)) == 0

def listener_pids(port):
    binary = '/usr/sbin/lsof' if Path('/usr/sbin/lsof').exists() else 'lsof'
    result = subprocess.run([binary, '-nP', f'-iTCP:{port}', '-sTCP:LISTEN', '-Fp'], capture_output=True, text=True, timeout=5)
    if result.returncode == 1 and not result.stdout:
        return []
    assert result.returncode == 0, result.stderr
    return [int(line[1:]) for line in result.stdout.splitlines() if line.startswith('p')]

def listening(port):
    return bool(listener_pids(port))

class Fixture(Live):
    def __init__(self, caddy, compose):
        super().__init__('r', caddy, compose)
        self.apps = []
    def cli(self, path, *args, check=False, timeout=25):
        return super().cli(path, *args, check=check, timeout=timeout)
    def app(self, label, delay=0):
        path = self.root / label
        path.mkdir()
        (path / 'service.py').write_text(SERVICE)
        manifest = {'version': 1, 'project': 'review', 'env': {'STOP_DELAY': str(delay)},
                    'services': {'web': {'command': f"python3 '{path / 'service.py'}'", 'http': {'healthcheck': '/ready'}}}}
        (path / 'parcela.yaml').write_text(json.dumps(manifest))
        identity = self.register(path, 'preview', label)['id']
        app = {'path': path, 'id': identity, 'host': f'{label}-review.localhost', 'runtime': self.state / 'environments' / identity,
               'port': free_port(), 'manifest': manifest}
        self.apps.append(app)
        self.ports.append(app['port'])
        return app
    def start(self, app, compose=None):
        owner = self.spawn(app['path'], 'up', '--process-compose', compose or self.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
        app['owner'] = owner
        wait_for(lambda: self.probe(app)[0] == 200)
        return owner
    def start_pending(self, app):
        app['manifest']['env']['READINESS_GATED'] = 'yes'
        (app['path'] / 'parcela.yaml').write_text(json.dumps(app['manifest']))
        owner = self.spawn(app['path'], 'up', '--process-compose', self.compose, '--backend-port', str(app['port']), '--readiness-timeout', '30')
        app['owner'] = owner
        def recorded():
            pid = int((app['runtime'] / 'service-started').read_text())
            record = json.loads((app['runtime'] / 'owner.json').read_text())
            return record['phase'] == 'starting' and record['native']['socket'] and any(process['pid'] == pid for process in record['processes'])
        wait_for(recorded)
        assert self.session(app)['status'] == 'starting'
        assert self.probe(app)[0] == 503
        return owner
    def gated_compose(self):
        wrapper = self.root / 'pc-gate'
        wrapper.write_text('''#!/usr/bin/env python3
import os, sys, time
from pathlib import Path
root = Path(__file__).parent
if sys.argv[-1] == 'down':
    with (root / 'native-stops').open('a') as log:
        log.write('down\\n')
if 'list' in sys.argv and (root / 'gate-enabled').exists():
    (root / 'gate-entered').write_text('entered')
    time.sleep(1)
os.execv(''' + repr(self.compose) + ''', [''' + repr(self.compose) + ''', *sys.argv[1:]])
''')
        wrapper.chmod(0o700)
        return str(wrapper)
    def session(self, app):
        with sqlite3.connect(f'file:{self.state / "registry.sqlite3"}?mode=ro', uri=True) as db:
            db.row_factory = sqlite3.Row
            return dict(db.execute('SELECT * FROM sessions WHERE environment_id=?', (app['id'],)).fetchone())
    def probe(self, app):
        code, _, body = self.fetch(app['host'], timeout=2)
        return code, body.decode()
    def caddy_config(self):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(3)
            stream.connect(json.loads((self.state / 'proxy/caddy.json').read_text())['admin']['listen'].removeprefix('unix/'))
            stream.sendall(b'GET /config/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n')
            with HTTPResponse(stream) as response:
                response.begin()
                assert response.status == 200
                return json.loads(response.read())
    def pc_down(self, app):
        context = json.loads((app['runtime'] / 'context.json').read_text())
        assert context['session']['environment_id'] == app['id']
        token = ['--token-file', str(app['runtime'] / 'pc.token')] if (app['runtime'] / 'pc.token').exists() else []
        return subprocess.run([self.compose, *token, '--use-uds', '--unix-socket', str(socket_path(app, 'compose')), '--log-file', str(self.root / 'native-client.log'), 'down'],
                              capture_output=True, text=True, timeout=20)
    def close(self):
        for app in self.apps:
            if (app['runtime'] / 'context.json').exists() and socket_path(app, 'compose').exists():
                try:
                    self.cli(app['path'], 'down')
                    if socket_path(app, 'compose').exists(): self.pc_down(app)
                except (OSError, subprocess.TimeoutExpired): pass
        super().close()
        for app in self.apps:
            if (app['runtime'] / 'context.json').exists():
                for kind in ('control', 'compose'):
                    path = socket_path(app, kind)
                    if path.is_socket() and not socket_live(path): path.unlink()
                try: socket_path(app, 'control').parent.rmdir()
                except OSError: pass

def owner_crash(f):
    app = f.app('crash')
    f.start_proxy()
    owner = f.start(app)
    owner.kill()
    assert owner.wait(timeout=5) == -signal.SIGKILL
    status = json.loads(f.cli(app['path'], 'status').stdout)
    assert status['state'] == 'unknown'
    wait_for(lambda: f.probe(app)[0] == 503, timeout=10)
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
        code, body = f.probe(app)
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
    assert not socket_path(app, 'compose').exists(), 'normal shutdown must remove its native socket'
    assert owner.wait(timeout=5) == 0
    retry = f.cli(app['path'], 'up', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
    assert retry.returncode == 0, retry.stderr
    return {'down_exit': down.returncode, 'shutdown_completed': True, 'subsequent_up_exit': retry.returncode}

def duplicate_stop(f):
    app = f.app('duplicate', delay=3)
    f.start_proxy()
    owner = f.start(app)
    command = [str(BINARY), '--path', str(app['path']), '--state-dir', str(f.state), 'down']
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
    assert f.probe(app)[0] == 200, 'failed preflight must preserve the healthy route'
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
        wait_for(lambda: f.probe(app)[0] == 200)
        assert f.session(app)['token'] == old['token']
        assert f.session(app)['generation'] == old['generation']
    return {'application_owners_preserved': True, 'routes_recovered': 2, 'application_generations_preserved': True}

def lost_stop_reply(f):
    app = f.app('lost', delay=2)
    f.start_proxy()
    owner = f.start(app)
    session = f.session(app)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(socket_path(app, 'control')))
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
    service = json.loads(f.probe(app)[1])
    owner.kill()
    owner.wait(timeout=5)
    recovered = f.cli(app['path'], 'recover', '--detach')
    assert recovered.returncode == 0, recovered.stderr
    after = f.session(app)
    assert after['token'] == before['token'] and after['generation'] == before['generation']
    assert json.loads(f.probe(app)[1])['pid'] == service['pid'], 'recovery must not replay or replace service processes'
    return {'controller_recovered': True, 'service_pid_preserved': True, 'generation_preserved': True}

def up_recovers_surviving_graph(f):
    app = f.app('upreattach')
    f.start_proxy()
    owner = f.start(app)
    before = f.session(app)
    service = json.loads(f.probe(app)[1])
    owner.kill()
    owner.wait(timeout=5)
    recovered = f.cli(app['path'], 'up', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']))
    assert recovered.returncode == 0, recovered.stderr
    assert 'taking back control of its running services' in recovered.stderr, recovered.stderr
    result = json.loads(recovered.stdout)
    assert result['recovered'] is True and result['detached'] is True and result['live']['state'] == 'ready', result
    after = f.session(app)
    assert (after['token'], after['generation'], after['status']) == (before['token'], before['generation'], 'ready')
    assert json.loads(f.probe(app)[1])['pid'] == service['pid'], 'up must reattach, not replay the service'
    status = json.loads(f.cli(app['path'], 'status').stdout)
    assert status['state'] == 'ready' and [process['name'] for process in status['processes']] == ['web'], status
    assert status['processes'][0]['state'] == 'running' and status['processes'][0]['pid'], status
    duplicate = f.cli(app['path'], 'up', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']))
    assert duplicate.returncode != 0 and json.loads(f.probe(app)[1])['pid'] == service['pid'], 'a live owner is never replaced'
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0 and json.loads(down.stdout)['state'] == 'stopped', down.stderr
    assert not listening(app['port'])
    return {'up_detach_recovered': True, 'service_pid_preserved': True, 'generation_preserved': True, 'duplicate_up_refused': True}

def up_foreground_recovers_surviving_graph(f):
    app = f.app('upforeground')
    f.start_proxy()
    owner = f.start(app)
    before = f.session(app)
    service = json.loads(f.probe(app)[1])
    owner.kill()
    owner.wait(timeout=5)
    recovered = f.spawn(app['path'], 'up', '--process-compose', f.compose, '--backend-port', str(app['port']))
    def reattached():
        live = json.loads(f.cli(app['path'], 'status').stdout)['live']
        return live and live['owner_pid'] == recovered.pid and live['state'] == 'ready'
    wait_for(reattached)
    after = f.session(app)
    assert (after['token'], after['generation']) == (before['token'], before['generation'])
    assert json.loads(f.probe(app)[1])['pid'] == service['pid'], 'up must reattach, not replay the service'
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0, down.stderr
    assert recovered.wait(timeout=20) == 0
    assert not listening(app['port'])
    return {'up_foreground_recovered': True, 'service_pid_preserved': True, 'foreground_exit': 0}

def guardian_death(f):
    app = f.app('guardian')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    f.proxy.kill()
    f.proxy.wait(timeout=5)
    wait_for(lambda: f.probe(app)[0] == 503, timeout=10)
    assert owner.poll() is None and listening(app['port'])
    status = json.loads(f.cli(app['path'], 'status').stdout)
    assert status['state'] == 'unhealthy' and not status['live']['route_published']
    f.start_proxy()
    wait_for(lambda: f.probe(app)[0] == 200)
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
        detached = f.cli(app['path'], 'recover', '--detach')
        assert detached.returncode != 0 and listening(app['port']), 'detached recovery must not bypass live native authentication'
    finally:
        token.write_text(original)
    return {'unauthenticated_recovery_refused': True, 'service_preserved': True}

def interrupted_stop(f):
    app = f.app('interrupted', delay=3)
    f.start_proxy()
    owner = f.start(app)
    session = f.session(app)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(socket_path(app, 'control')))
        client.sendall((json.dumps({'environment_id': app['id'], 'token': session['token'], 'generation': session['generation'], 'action': {'type': 'stop'}}) + '\n').encode())
    wait_for(lambda: (app['runtime'] / 'shutdown-started').exists())
    owner.kill()
    owner.wait(timeout=5)
    wait_for(lambda: (app['runtime'] / 'shutdown-completed').exists())
    wait_for(lambda: not listening(app['port']))
    retry = f.cli(app['path'], 'down')
    assert retry.returncode == 0, retry.stderr
    assert f.session(app)['status'] == 'stopped'
    assert f.probe(app)[0] == 503
    return {'interrupted_down_retry_exit': 0, 'shutdown_completed': True, 'route_withdrawn': True}


def ongoing_stop_retry(f):
    app = f.app('ongoing', delay=7)
    f.start_proxy()
    owner = f.start(app)
    session = f.session(app)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(socket_path(app, 'control')))
        client.sendall((json.dumps({'environment_id': app['id'], 'token': session['token'], 'generation': session['generation'], 'action': {'type': 'stop'}}) + '\n').encode())
    wait_for(lambda: (app['runtime'] / 'shutdown-started').exists())
    owner.kill()
    owner.wait(timeout=5)
    assert not (app['runtime'] / 'shutdown-completed').exists(), 'retry must start while native shutdown is ongoing'
    assert listening(app['port']), 'fixture graph must still be running at retry'
    retry = f.cli(app['path'], 'down')
    assert retry.returncode == 0, retry.stderr
    assert (app['runtime'] / 'shutdown-completed').exists(), 'successful retry must await native shutdown'
    assert not listening(app['port'])
    assert f.session(app)['status'] == 'stopped'
    assert f.session(app)['token'] == session['token']
    assert f.probe(app)[0] == 503
    return {'retry_during_native_shutdown_exit': 0, 'shutdown_completed': True, 'generation_preserved': True}


def concurrent_crash_stop(f):
    app = f.app('election', delay=2)
    wrapper = f.gated_compose()
    f.start_proxy()
    owner = f.start(app, str(wrapper))
    session = f.session(app)
    owner.kill()
    owner.wait(timeout=5)
    (f.root / 'gate-enabled').write_text('enabled')
    first = f.spawn(app['path'], 'down')
    wait_for(lambda: (f.root / 'gate-entered').exists())
    second = f.spawn(app['path'], 'down')
    first_code = first.wait(timeout=20)
    second_code = second.wait(timeout=20)
    first_log = (f.root / f'child-{f.children.index(first)}.log').read_text()
    second_log = (f.root / f'child-{f.children.index(second)}.log').read_text()
    assert first_code == 0, first_log
    assert second_code == 0, second_log
    assert f.session(app)['status'] == 'stopped'
    assert f.session(app)['token'] == session['token']
    assert (app['runtime'] / 'shutdown-completed').exists()
    assert not listening(app['port'])
    assert (f.root / 'native-stops').read_text().splitlines() == ['down'], 'only the elected controller may request native shutdown'
    return {'first_recovery_exit': first_code, 'contending_recovery_exit': second_code, 'same_generation_stopped': True, 'native_stop_requests': 1}


def recover_before_readiness(f):
    app = f.app('pending')
    f.start_proxy()
    owner = f.start_pending(app)
    session = f.session(app)
    pid = int((app['runtime'] / 'service-started').read_text())
    owner.kill()
    owner.wait(timeout=5)
    recovered = f.spawn(app['path'], 'recover')
    def starting_owner():
        status = f.cli(app['path'], 'status')
        live = json.loads(status.stdout).get('live')
        return live and live['owner_pid'] == recovered.pid and live['state'] == 'starting'
    wait_for(starting_owner)
    assert f.probe(app)[0] == 503, 'recovery must not advertise an unready service'
    (app['runtime'] / 'allow-ready').write_text('ready')
    wait_for(lambda: f.probe(app)[0] == 200)
    assert json.loads(f.probe(app)[1])['pid'] == pid, 'recovery must not replay the native service'
    assert f.session(app)['token'] == session['token']
    return {'pre_readiness_controller_recovered': True, 'unready_route': 503, 'service_pid_preserved': True}


def stop_before_readiness(f):
    app = f.app('pending-stop')
    f.start_proxy()
    owner = f.start_pending(app)
    session = f.session(app)
    owner.kill()
    owner.wait(timeout=5)
    down = f.cli(app['path'], 'down')
    assert down.returncode == 0, down.stderr
    assert f.session(app)['status'] == 'stopped'
    assert f.session(app)['token'] == session['token']
    assert (app['runtime'] / 'shutdown-completed').exists()
    assert not listening(app['port'])
    assert f.probe(app)[0] == 503
    return {'pre_readiness_stop_exit': 0, 'shutdown_completed': True, 'generation_preserved': True}


def allocation_without_launch_evidence(f):
    app = f.app('allocation')
    marker = app['path'] / 'owned-source-data'
    marker.write_text('preserve allocation fixture')
    wrapper = f.root / 'pc-allocation-gate'
    wrapper.write_text('''#!/usr/bin/env python3
import os, sys, time
from pathlib import Path
root = Path(__file__).parent
if sys.argv[1:] == ['version'] and Path(''' + repr(str(app['runtime'] / 'context.json')) + ''').exists() and (root / 'gate-enabled').exists():
    (root / 'gate-entered').write_text('before graph launch')
    deadline = time.monotonic() + 2
    while (root / 'gate-enabled').exists() and time.monotonic() < deadline:
        time.sleep(.01)
os.execv(''' + repr(f.compose) + ''', [''' + repr(f.compose) + ''', *sys.argv[1:]])
''')
    wrapper.chmod(0o700)
    f.start_proxy()
    (f.root / 'gate-enabled').write_text('enabled')
    owner = f.spawn(app['path'], 'up', '--process-compose', str(wrapper), '--backend-port', str(app['port']))
    wait_for(lambda: (f.root / 'gate-entered').exists())
    session = f.session(app)
    assert session['status'] == 'starting' and not (app['runtime'] / 'owner.json').exists()
    owner.kill()
    owner.wait(timeout=5)
    (f.root / 'gate-enabled').unlink()
    down = f.cli(app['path'], 'down')
    assert down.returncode != 0, 'missing launch evidence must not authorize lease release'
    retry = f.cli(app['path'], 'up', '--process-compose', str(wrapper), '--backend-port', str(app['port']))
    assert retry.returncode != 0, 'uncertain allocation must not be silently replayed'
    assert f.session(app)['token'] == session['token'] and f.session(app)['status'] == 'starting'
    assert marker.read_text() == 'preserve allocation fixture'
    assert not socket_path(app, 'compose').exists() and not listening(app['port'])
    assert f.probe(app)[0] == 503
    return {'unknown_allocation_refused': True, 'lease_retained': True, 'source_data_preserved': True}


def recover_racing_stop(f):
    app = f.app('mixed')
    wrapper = f.gated_compose()
    f.start_proxy()
    owner = f.start(app, wrapper)
    session = f.session(app)
    pid = (app['runtime'] / 'service-started').read_text()
    owner.kill()
    owner.wait(timeout=5)
    (f.root / 'gate-enabled').write_text('enabled')
    recovered = f.spawn(app['path'], 'recover')
    wait_for(lambda: (f.root / 'gate-entered').exists())
    stopper = f.spawn(app['path'], 'down')
    try:
        stop_code = stopper.wait(timeout=10)
        stop_log = (f.root / f'child-{f.children.index(stopper)}.log').read_text()
        assert stop_code == 0, stop_log
        assert recovered.wait(timeout=5) == 0
        assert f.session(app)['status'] == 'stopped'
        assert f.session(app)['token'] == session['token']
        assert (app['runtime'] / 'service-started').read_text() == pid
        assert (f.root / 'native-stops').read_text().splitlines() == ['down']
        assert not listening(app['port'])
    finally:
        (f.root / 'gate-enabled').unlink()
    return {'mixed_stop_exit': 0, 'recovery_controller_exit': 0, 'native_stop_requests': 1, 'no_service_replay': True}


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
    path = socket_path(app, 'compose')
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
            assert socket_path(app, 'control').exists(), 'refusal must preserve original control evidence'
            wait_for(lambda: f.probe(app)[0] == 503)
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
        wait_for(lambda: f.probe(app)[0] == 503)
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
                assert f.probe(app)[0] == 200, 'lease renewal must keep new requests routed'
    assert owner.poll() is None
    assert f.caddy_config() == loaded, 'lease renewal must not change actual Caddy config'
    return {'stream_events': 12, 'lease_windows_crossed': True, 'config_unchanged': True}


def occupied_replacement_port(f):
    app = f.app('occupied')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    with socket.socket() as foreign:
        foreign.bind(('127.0.0.1', 0))
        foreign.listen()
        port = foreign.getsockname()[1]
        restart = f.cli(app['path'], 'restart', '--detach', '--process-compose', f.compose, '--backend-port', str(port))
        assert restart.returncode != 0
        assert owner.poll() is None, 'occupied replacement preflight must preserve original owner'
        assert f.session(app)['token'] == original['token']
        assert f.session(app)['generation'] == original['generation']
        assert f.probe(app)[0] == 200, 'occupied replacement must preserve original HTTPS'
        with socket.create_connection(('127.0.0.1', port), timeout=2):
            pass
    return {'restart_exit': restart.returncode, 'original_owner_alive': True, 'https_code': 200, 'foreign_listener_preserved': True}

def invalid_replacement_port(f):
    app = f.app('invalid')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    for port in [0, 80, f.https]:
        restart = f.cli(app['path'], 'restart', '--detach', '--process-compose', f.compose, '--backend-port', str(port))
        assert restart.returncode != 0, f'invalid replacement port {port} must be refused'
        assert owner.poll() is None, f'invalid replacement port {port} stopped original owner'
        assert f.session(app)['token'] == original['token']
        assert f.probe(app)[0] == 200
    return {'invalid_endpoints_refused': 3, 'original_owner_alive': True, 'https_code': 200}

def restart_owned_port(f):
    app = f.app('reuse')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    restart = f.cli(app['path'], 'restart', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']))
    assert restart.returncode == 0, restart.stderr
    assert owner.wait(timeout=5) == 0
    current = f.session(app)
    assert current['token'] != original['token']
    assert current['generation'] == original['generation'] + 1
    assert current['backend_port'] == app['port']
    assert f.probe(app)[0] == 200
    return {'restart_exit': 0, 'owned_endpoint_reused': True, 'new_generation': current['generation'], 'https_code': 200}

def leased_replacement_port(f):
    app = f.app('leased')
    other = f.app('reserved')
    f.start_proxy()
    owner = f.start(app)
    other_owner = f.start(other)
    original = f.session(app)
    other_owner.kill()
    other_owner.wait(timeout=5)
    assert f.pc_down(other).returncode == 0
    wait_for(lambda: not listening(other['port']))
    assert f.session(other)['status'] != 'stopped'
    restart = f.cli(app['path'], 'restart', '--detach', '--process-compose', f.compose, '--backend-port', str(other['port']))
    assert restart.returncode != 0
    assert owner.poll() is None, 'reserved replacement must preserve original owner'
    assert f.session(app)['token'] == original['token']
    assert f.probe(app)[0] == 200
    return {'reserved_unbound_endpoint_refused': True, 'original_owner_alive': True, 'https_code': 200}

def detached_recover_running(f):
    app = f.app('rundetach')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    before = json.loads(f.probe(app)[1])
    owner.kill()
    owner.wait(timeout=5)
    recover = f.cli(app['path'], 'recover', '--detach', timeout=25)
    assert recover.returncode == 0, recover.stderr
    result = json.loads(recover.stdout)
    assert result['live']['state'] == 'ready' and result['detached'] is True
    current = f.session(app)
    assert current['token'] == original['token'] and current['generation'] == original['generation']
    assert json.loads(f.probe(app)[1])['pid'] == before['pid'], 'detached recovery must not replay services'
    assert f.cli(app['path'], 'down').returncode == 0
    assert not listening(app['port'])
    return {'recover_exit': 0, 'state': 'ready', 'service_pid_preserved': True, 'same_generation': True}

def recovered_owner_initialization(f):
    app = f.app('initialize')
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    owner.kill()
    owner.wait(timeout=5)
    tools = f.root / 'tools'
    tools.mkdir()
    ps = tools / 'ps'
    ps.write_text(f"""#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
root = Path({str(f.root)!r})
record_path = Path({str(app['runtime'] / 'owner.json')!r})
if '-A' in sys.argv and not (root / 'refresh-paused').exists():
    record = json.loads(record_path.read_text())
    if record['phase'] == 'starting' and record['owner']['pid'] != {owner.pid}:
        (root / 'refresh-paused').write_text('paused before recovered control admission')
        time.sleep(1)
os.execv({shutil.which('ps')!r}, ['ps', *sys.argv[1:]])
""")
    ps.chmod(0o700)
    recovery = f.spawn(app['path'], 'recover', '--detach', env={**os.environ, 'PATH': str(tools) + os.pathsep + os.environ['PATH']})
    wait_for(lambda: (f.root / 'refresh-paused').exists())
    record = json.loads((app['runtime'] / 'owner.json').read_text())
    assert record['phase'] == 'starting'
    current = f.session(app)
    assert current['status'] == 'starting', 'new recovered owner must not inherit former ready intent before initialization'
    assert current['token'] == original['token'] and current['generation'] == original['generation']
    assert recovery.wait(timeout=20) == 0
    assert f.probe(app)[0] == 200
    return {'initializing_state': 'starting', 'same_generation': True, 'https_code': 200}

def detached_recover_stopping(f):
    return detached_stopped_recovery(f, False)

def detached_recover_quiescent(f):
    return detached_stopped_recovery(f, True)

def detached_stopped_recovery(f, finish_first):
    app = f.app('stopdetach', delay=7)
    f.start_proxy()
    owner = f.start(app)
    original = f.session(app)
    with socket.socket(socket.AF_UNIX) as control:
        control.settimeout(5)
        control.connect(str(socket_path(app, 'control')))
        control.sendall((json.dumps({'environment_id': app['id'], 'generation': original['generation'], 'token': original['token'], 'action': {'type': 'stop'}}) + '\n').encode())
        with control.makefile('rb') as response:
            reply = json.loads(response.readline())
        assert reply['result']['stop_accepted'] is True
    wait_for(lambda: (app['runtime'] / 'shutdown-started').exists())
    owner.kill()
    owner.wait(timeout=5)
    if finish_first:
        wait_for(lambda: (app['runtime'] / 'shutdown-completed').exists())
        wait_for(lambda: not listening(app['port']))
        wait_for(lambda: f.pc_down(app).returncode != 0)
    recover = f.cli(app['path'], 'recover', '--detach', timeout=25)
    assert recover.returncode == 0, recover.stderr
    result = json.loads(recover.stdout)
    assert result['state'] == 'stopped' and result['recovered'] is True and result['detached'] is True
    current = f.session(app)
    assert current['token'] == original['token'] and current['generation'] == original['generation']
    assert current['status'] == 'stopped'
    assert (app['runtime'] / 'shutdown-completed').exists()
    assert not listening(app['port'])
    return {'recover_exit': 0, 'state': 'stopped', 'same_generation': True, 'finished_before_recover': finish_first}


def crash_whole_tree(f, app):
    f.start_proxy()
    owner = f.start(app)
    before = f.session(app)
    service = int((app['runtime'] / 'service-started').read_text())
    wait_for(lambda: any(process['pid'] == service for process in json.loads((app['runtime'] / 'owner.json').read_text())['processes']))
    record = json.loads((app['runtime'] / 'owner.json').read_text())
    owner.kill()
    owner.wait(timeout=5)
    for process in record['processes']:
        try: os.kill(process['pid'], signal.SIGKILL)
        except ProcessLookupError: pass
    wait_for(lambda: not listening(app['port']))
    assert socket_path(app, 'compose').exists() and socket_path(app, 'control').exists(), 'a crash leaves stale sockets'
    assert f.session(app)['status'] == 'ready'
    return before

def crashed_tree_down(f):
    app = f.app('crashdown')
    before = crash_whole_tree(f, app)
    first = f.cli(app['path'], 'down')
    assert first.returncode == 0, first.stderr
    assert json.loads(first.stdout)['state'] == 'stopped'
    second = f.cli(app['path'], 'down')
    assert second.returncode == 0 and json.loads(second.stdout)['already_stopped'] is True, second.stderr
    assert not socket_path(app, 'compose').exists() and not socket_path(app, 'control').exists()
    assert f.probe(app)[0] == 503
    retry = f.cli(app['path'], 'up', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
    assert retry.returncode == 0, retry.stderr
    after = f.session(app)
    assert after['generation'] == before['generation'] + 1 and after['status'] == 'ready'
    assert f.probe(app)[0] == 200
    assert f.cli(app['path'], 'down').returncode == 0
    return {'first_down': 'stopped', 'second_down': 'already_stopped', 'next_generation': after['generation'], 'https_code': 200}

def crashed_tree_up(f):
    app = f.app('crashup')
    before = crash_whole_tree(f, app)
    retry = f.cli(app['path'], 'up', '--detach', '--process-compose', f.compose, '--backend-port', str(app['port']), '--readiness-timeout', '10')
    assert retry.returncode == 0, retry.stderr
    after = f.session(app)
    assert after['generation'] == before['generation'] + 1 and after['status'] == 'ready'
    assert f.probe(app)[0] == 200
    assert f.cli(app['path'], 'down').returncode == 0
    return {'up_after_crash_exit': 0, 'next_generation': after['generation'], 'https_code': 200}

CASES = [owner_crash, graceful_stop, duplicate_stop, restart_preflight, proxy_outage, lost_stop_reply,
         recover_surviving_graph, up_recovers_surviving_graph, up_foreground_recovers_surviving_graph, guardian_death, wrong_native_token, interrupted_stop, completed_stop_receipt,
         replaced_native_socket, replaced_process_birth, long_stream_lease, ongoing_stop_retry, concurrent_crash_stop,
         recover_before_readiness, stop_before_readiness, allocation_without_launch_evidence, recover_racing_stop,
         occupied_replacement_port, invalid_replacement_port, restart_owned_port, leased_replacement_port, detached_recover_running,
         detached_recover_stopping, detached_recover_quiescent, recovered_owner_initialization,
         crashed_tree_down, crashed_tree_up]


def run(caddy, compose, cases=None):
    os.umask(0o077)
    selected = [case for case in CASES if not cases or case.__name__ in cases]
    assert selected, f'no recovery case matches {cases}'
    records = []
    for case in selected:
        record = {'case': case.__name__}
        try:
            with Fixture(caddy, compose) as fixture:
                record['fixture'] = str(fixture.root)
                record['evidence'] = case(fixture)
        except Exception:
            record['error'] = traceback.format_exc()
        records.append(record)
        print(f"  recovery {case.__name__}: {'FAIL' if 'error' in record else 'ok'}", flush=True)
    failed = [record['case'] for record in records if 'error' in record]
    assert not failed, {'failed': failed, 'cases': records}
    return {'cases': records}
