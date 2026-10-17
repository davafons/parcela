"""Disposable live backend acceptance. No standard ports, trust changes or source edits."""
import argparse
import json
import os
import pty
from pathlib import Path
import signal
import socket
import ssl
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / 'target/debug/parcela'
FIXTURE = ROOT / 'tests/fixtures/http_service.py'

def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]

def on_terminal(argv, timeout=15):
    """Run argv with stdout on a pseudo-terminal, where Parcela prints human output."""
    primary, secondary = pty.openpty()
    try:
        process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=secondary, stderr=subprocess.DEVNULL,
                                   env={**os.environ, 'NO_COLOR': '1'})
    finally:
        os.close(secondary)
    output = b''
    try:
        while chunk := os.read(primary, 65536):
            output += chunk
    except OSError:
        pass
    finally:
        os.close(primary)
    assert process.wait(timeout=timeout) == 0, (argv, output)
    return output.decode().replace('\r\n', '\n')

def control_socket(runtime):
    return Path(json.loads((runtime / 'context.json').read_text())['sockets']['control'])

def wait_for(check, processes=(), timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        for process in processes:
            if process.poll() is not None:
                raise RuntimeError(f'process exited early: {process.args}, code {process.returncode}')
        try:
            result = check()
            if result:
                return result
        except (OSError, ValueError, subprocess.CalledProcessError):
            pass
        time.sleep(.1)
    raise RuntimeError('acceptance probe timed out')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', required=True)
    args = parser.parse_args()
    children, handles = [], []
    with tempfile.TemporaryDirectory(prefix='parcela-live-', dir='/tmp') as temporary:
        root = Path(temporary).resolve()
        state = root / 'state'
        https = free_port()
        def cli(path, *argv, check=True):
            return subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                                  capture_output=True, text=True, check=check, timeout=15)
        def human(path, *argv):
            return on_terminal([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv])
        def spawn(path, *argv):
            log = open(root / f'process-{len(children)}.log', 'w+')
            handles.append(log)
            process = subprocess.Popen([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv], stdout=log, stderr=log)
            children.append(process)
            return process
        def request(host, path='/ready', extra_headers='', raw=False, request_host=None):
            ca = state / 'proxy/data/pki/authorities/local/root.crt'
            context = ssl.create_default_context(cafile=str(ca))
            with context.wrap_socket(socket.create_connection(('127.0.0.1', https), timeout=3), server_hostname=host) as sock:
                sock.sendall(f'GET {path} HTTP/1.1\r\nHost: {request_host or host}\r\nConnection: close\r\n{extra_headers}\r\n'.encode())
                if path == '/ws':
                    return sock.recv(8192)
                data = b''
                while True:
                    part = sock.recv(8192)
                    if not part:
                        break
                    data += part
                if raw:
                    return data
                head, body = data.split(b'\r\n\r\n', 1)
                if not head.startswith(b'HTTP/1.1 200'):
                    raise ValueError(f'route not ready: {head!r}')
                return json.loads(body)
        try:
            copies = []
            for name, role in [('primary', 'primary'), ('auth', 'preview'), ('compose', 'preview')]:
                path = root / name
                path.mkdir()
                (path / 'parcela.yaml').write_text(f'''version: 1
project: {{id: live, name: live}}
default_service: web
services: {{web: {{readiness_path: /ready}}}}
process_compose:
  version: '0.5'
  processes:
    web:
      command: python3 {FIXTURE}
      availability:
        restart: 'no'
    worker:
      command: python3 -c 'import time; print("cli-worker-ready", flush=True); time.sleep(600)'
''')
                adopted = json.loads(cli(path, 'workspace', 'adopt', '--role', role, '--label', name).stdout)
                copies.append((path, adopted['workspace']['id']))
            proxy = spawn(root, 'proxy', 'run', '--caddy', args.caddy, '--port', str(https))
            wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
            primary = spawn(copies[0][0], 'run', '--', 'python3', str(FIXTURE), 'literal argument with spaces', '--flag')
            preview = spawn(copies[1][0], 'run', '--', 'python3', str(FIXTURE))
            wait_for(lambda: request('live.localhost'), [proxy, primary, preview])
            wait_for(lambda: request('auth-live.localhost'), [proxy, primary, preview])
            assert request('live.localhost')['id'] == copies[0][1]
            assert request('auth-live.localhost')['id'] == copies[1][1]
            assert request('live.localhost')['argv'] == ['literal argument with spaces', '--flag']
            headers = request('live.localhost', extra_headers='X-Forwarded-Proto: http\r\nX-Forwarded-Host: evil.example\r\n')['headers']
            assert headers['X-Forwarded-Proto'] == 'https', headers
            assert headers['X-Forwarded-Host'] == 'live.localhost', headers
            assert b'data: ready' in request('live.localhost', '/sse', raw=True)
            websocket = request('live.localhost', '/ws', extra_headers='Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n')
            assert websocket.startswith(b'HTTP/1.1 101'), websocket
            # Unknown HTTP Host with a valid SNI certificate cannot select another route.
            assert b'404' in request('live.localhost', request_host='unknown.localhost', raw=True).split(b'\r\n', 1)[0]
            duplicate = cli(copies[0][0], 'run', '--', 'python3', str(FIXTURE), check=False)
            assert duplicate.returncode != 0 and primary.poll() is None
            primary.send_signal(signal.SIGTERM)
            assert primary.wait(timeout=10) == 143
            assert request('auth-live.localhost')['id'] == copies[1][1]
            assert b'503' in request('auth-live.localhost', request_host='live.localhost', raw=True).split(b'\r\n', 1)[0]
            assert cli(copies[0][0], 'run', '--', 'python3', '-c', 'raise SystemExit(7)', check=False).returncode == 7
            with socket.socket() as foreign:
                foreign.bind(('127.0.0.1', 0))
                foreign.listen()
                port = foreign.getsockname()[1]
                collision = cli(copies[0][0], 'run', '--backend-port', str(port), '--', 'python3', str(FIXTURE), check=False)
                assert collision.returncode != 0
                assert foreign.getsockname()[1] == port
            compose = spawn(copies[2][0], 'up', '--process-compose', args.process_compose)
            wait_for(lambda: request('compose-live.localhost'), [proxy, preview, compose])
            assert request('compose-live.localhost')['id'] == copies[2][1]
            status = json.loads(cli(copies[2][0], 'status').stdout)
            assert status['state'] == 'ready' and status['live_health_verified']
            assert status['live']['http_ready'] and status['live']['listener_owned']
            services = json.loads(cli(copies[2][0], 'services').stdout)['services']
            assert {item['name']: item['status'] for item in services} == {'web': 'Running', 'worker': 'Running'}
            wait_for(lambda: 'cli-worker-ready' in cli(copies[2][0], 'logs', 'worker', '--tail', '10').stdout, [compose])
            assert 'SERVICE' in human(copies[2][0], 'services')
            assert cli(copies[2][0], 'logs', 'unknown', check=False).returncode != 0
            assert cli(copies[1][0], 'restart', '--detach', '--process-compose', args.process_compose, check=False).returncode != 0
            assert preview.poll() is None
            assert cli(copies[2][0], 'restart', '--detach', '--readiness-timeout', '0', check=False).returncode == 2
            assert cli(copies[2][0], 'restart', '--detach', '--backend-port', '80', check=False).returncode == 2
            assert compose.poll() is None
            assert cli(copies[2][0], 'url').stdout.strip() == status['recorded_session']['public_url']
            runtime = state / 'environments' / copies[2][1]
            (runtime / 'data' / 'retain.txt').write_text('preserve application data')
            (runtime / 'unhealthy').touch()
            unhealthy = json.loads(cli(copies[2][0], 'status').stdout)
            assert unhealthy['state'] == 'unhealthy' and unhealthy['recorded_session']['status'] == 'ready'
            assert unhealthy['live_health_verified'] and not unhealthy['live']['http_ready']
            (runtime / 'unhealthy').unlink()
            stopped = json.loads(cli(copies[2][0], 'down').stdout)
            assert stopped['state'] == 'stopped' and compose.wait(timeout=10) == 0
            assert not control_socket(runtime).exists()
            assert b'503' in request('auth-live.localhost', request_host='compose-live.localhost', raw=True).split(b'\r\n', 1)[0]
            assert json.loads(cli(copies[2][0], 'down').stdout)['already_stopped']
            first = json.loads(cli(copies[2][0], 'up', '--detach', '--process-compose', args.process_compose).stdout)
            assert first['detached'] and first['live']['state'] == 'ready'
            assert first['workspace']['generation'] == first['session']['generation']
            duplicate = cli(copies[2][0], 'up', '--detach', '--process-compose', args.process_compose, check=False)
            assert duplicate.returncode != 0
            second = json.loads(cli(copies[2][0], 'restart', '--detach', '--process-compose', args.process_compose).stdout)
            assert second['session']['generation'] > first['session']['generation']
            assert second['session']['token'] != first['session']['token']
            assert request('compose-live.localhost')['generation'] == str(second['session']['generation'])
            with socket.socket(socket.AF_UNIX) as stale:
                stale.settimeout(3)
                stale.connect(str(control_socket(runtime)))
                stale.sendall((json.dumps({'environment_id': copies[2][1], 'token': first['session']['token'],
                                          'generation': first['session']['generation'], 'action': {'type': 'stop'}}) + '\n').encode())
                reply = json.loads(stale.makefile().readline())
                assert 'stale' in reply['error']
            assert request('compose-live.localhost')['id'] == copies[2][1]
            inventory = json.loads(cli(root, 'status', '--all').stdout)['workspaces']
            assert {row['workspace']['label']: row['state'] for row in inventory} == {'primary': 'stopped', 'auth': 'ready', 'compose': 'ready'}
            assert 'compose' in human(root, 'status', '--all')
            selected = subprocess.run([str(BINARY), '--state-dir', str(state), '--workspace', copies[2][1], 'status'],
                                      capture_output=True, text=True, check=True, timeout=15)
            assert json.loads(selected.stdout)['workspace']['id'] == copies[2][1]
            assert json.loads(cli(copies[2][0], 'stop').stdout)['state'] == 'stopped'
            assert (runtime / 'data' / 'retain.txt').read_text() == 'preserve application data'
            with socket.socket(socket.AF_UNIX) as foreign_control:
                foreign_control.bind(str(control_socket(runtime)))
                failed = cli(copies[2][0], 'up', '--process-compose', args.process_compose, check=False)
                assert failed.returncode != 0 and control_socket(runtime).exists()
            control_socket(runtime).unlink()
            failed = cli(copies[2][0], 'up', '--detach', '--process-compose', '/nonexistent/parcela-test-compose', check=False)
            assert failed.returncode != 0
            assert json.loads(cli(copies[2][0], 'status').stdout)['state'] == 'stopped'
            assert not control_socket(runtime).exists()
            assert request('auth-live.localhost')['id'] == copies[1][1]
            print(json.dumps({'verified': ['owned Caddy TLS/CA without trust', 'two simultaneous routes', 'literal argv', 'forwarded scheme/host spoof rejection',
                          'SSE', 'WebSocket upgrade', 'unknown host rejection', 'duplicate ownership', 'SIGTERM', 'exit status', 'foreign port safety',
                          'Process Compose web+worker private UDS', 'independent teardown', 'owner-mediated down/stop', 'detached startup/readiness',
                           'restart generation', 'unhealthy probe separate from recorded ready', 'stale command rejection', 'live/aggregate status and UUID targeting', 'service/log inspection',
                           'data retention', 'foreign control socket refusal', 'failed detached startup cleanup'], 'test_ports': {'https': https}, 'standard_ports_verified': False}))
        except Exception:
            for handle in handles:
                handle.flush()
                handle.seek(0)
                print(handle.read())
            raise
        finally:
            for path, _ in locals().get('copies', []):
                cli(path, 'down', check=False)
            for process in reversed(children):
                if process.poll() is None:
                    process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=12)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            for handle in handles:
                handle.close()

if __name__ == '__main__':
    main()
