"""Opt-in real Shirabe disposable web acceptance, using existing dependencies only."""
import argparse
import http.client
from http.cookies import SimpleCookie
import json
from pathlib import Path
import signal
import socket
import ssl
import subprocess
import tempfile
from shirabe_snapshot import snapshot
from check_live import free_port, wait_for

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / 'target/debug/parcela'

class LocalHTTPS(http.client.HTTPSConnection):
    def connect(self):
        self.sock = self._context.wrap_socket(socket.create_connection(('127.0.0.1', self.port), timeout=5), server_hostname=self.host)

def compose_processes(process_compose, runtime, log_path):
    socket_path = json.loads((runtime / 'context.json').read_text()).get('sockets', {}).get('compose', runtime / 'pc.sock')
    output = subprocess.check_output([process_compose, '--use-uds', '--unix-socket', str(socket_path), '--token-file', str(runtime / 'pc.token'), '--log-file', str(log_path), 'process', 'list', '--output', 'json'], text=True, stderr=subprocess.DEVNULL)
    records = json.loads(output)
    if isinstance(records, dict):
        records = records.get('data', records.get('processes', records))
    if not isinstance(records, list):
        raise ValueError(f'Unexpected Process Compose state: {records}')
    selected = {record['name']: record for record in records}
    for name in ['web', 'css', 'jobs']:
        if selected[name]['status'] != 'Running':
            raise ValueError(f'Native graph component not running: {selected[name]}')
    return selected

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', required=True)
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', help='Also enable and exercise native CSS/jobs graph in the disposable copies')
    args = parser.parse_args()
    children, logs = [], []
    with tempfile.TemporaryDirectory(prefix='ps-', dir='/tmp') as directory:
        root = Path(directory).resolve()
        state = root / 'state'
        https, http_port = free_port(), free_port()
        copies = []
        def cli(path, *argv, timeout=180, check=True):
            output = subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                                    capture_output=True, text=True, timeout=timeout)
            if check and output.returncode:
                raise RuntimeError(f'{argv}: {output.returncode}\n{output.stdout[-12000:]}\n{output.stderr[-12000:]}')
            return output
        def spawn(path, *argv):
            log = open(root / f'child-{len(children)}.log', 'w+')
            logs.append(log)
            child = subprocess.Popen([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv], stdout=log, stderr=log)
            children.append(child)
            return child
        def request(host, method='GET', path='/__parcela_probe', cookie=None, csrf=None, origin=None):
            context = ssl.create_default_context(cafile=str(state / 'proxy/data/pki/authorities/local/root.crt'))
            connection = LocalHTTPS(host, https, context=context, timeout=5)
            headers = {'User-Agent': 'Mozilla/5.0 Firefox/128.0', 'Accept': 'application/json'}
            if cookie:
                headers['Cookie'] = cookie
            if csrf:
                headers['X-CSRF-Token'] = csrf
            if origin:
                headers['Origin'] = origin
            connection.request(method, path, headers=headers)
            response = connection.getresponse()
            body = response.read()
            response_headers = response.getheaders()
            connection.close()
            return response.status, body, response_headers
        try:
            for label, role in [('primary', 'primary'), ('auth', 'preview')]:
                copy = root / label
                hashes = snapshot(args.source, copy)
                if args.process_compose:
                    manifest = copy / 'parcela.yaml'
                    manifest.write_text(manifest.read_text().replace('disabled: true', 'disabled: false').replace('SHIRABE_RUN_JOBS: "0"', 'SHIRABE_RUN_JOBS: "1"'))
                # Test-only controller exercises the actual app Authentication concern and Rails middleware.
                (copy / 'app/controllers/parcela_probe_controller.rb').write_bytes((ROOT / 'tests/fixtures/shirabe_probe_controller.rb').read_bytes())
                with (copy / 'config/routes.rb').open('a') as routes:
                    routes.write('\nRails.application.routes.append do\n  get "__parcela_probe", to: "parcela_probe#show"\n  post "__parcela_probe", to: "parcela_probe#create"\nend\n')
                lock = subprocess.run(['bundle', 'lock', '--local'], cwd=copy, capture_output=True, text=True, timeout=60)
                if lock.returncode:
                    raise RuntimeError(f'Existing dependencies unavailable; no installation attempted:\n{lock.stderr}')
                adopted = json.loads(cli(copy, 'workspace', 'adopt', '--role', role, '--label', label).stdout)
                # Explicit project task: fresh schema/seed recipe, never a live database copy.
                cli(copy, 'task', 'prepare')
                cli(copy, 'task', 'prepare')  # Same input fingerprints: no destructive re-preparation.
                copies.append((copy, adopted['workspace']['id'], hashes))
            proxy = spawn(root, 'proxy', 'run', '--caddy', args.caddy, '--test-profile', '--https-port', str(https), '--http-port', str(http_port))
            wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
            servers = []
            for copy, _, _ in copies:
                if args.process_compose:
                    servers.append(spawn(copy, 'up', '--readiness-timeout', '90', '--process-compose', args.process_compose))
                else:
                    servers.append(spawn(copy, 'run', '--readiness-timeout', '90', '--', 'bin/rails', 'server', '-b', '127.0.0.1'))
            native = []
            if args.process_compose:
                for _, environment, _ in copies:
                    native.append(wait_for(lambda: compose_processes(args.process_compose, state / 'environments' / environment, root / 'pc-client.log'), [proxy, *servers], timeout=45))
            probes = []
            for host, (copy, environment, _) in zip(['shirabe.localhost', 'auth-shirabe.localhost'], copies):
                wait_for(lambda: request(host, path='/up')[0] == 200, [proxy, *servers], timeout=100)
                status, body, headers = request(host)
                assert status == 200, body.decode(errors='replace')[:5000]
                probe = json.loads(body)
                assert probe['dictionary_entries'] == 0 and probe['sessions'] == 0, probe
                assert probe['scheme'] == 'https' and probe['host'] == host and probe['environment_id'] == environment, probe
                assert probe['route_url'].startswith(f'https://{host}:{https}'), probe
                assert probe['mailer_url_options']['host'] == host and probe['mailer_url_options']['port'] == https, probe
                assert probe['cable_origins'] == [f'https://{host}:{https}'], probe
                cookies = SimpleCookie()
                for key, value in headers:
                    if key.lower() == 'set-cookie':
                        cookies.load(value)
                cookie_header = '; '.join(f'{key}={morsel.value}' for key, morsel in cookies.items())
                status, _, _ = request(host, 'POST', cookie=cookie_header, csrf=probe['csrf'], origin='https://foreign.example')
                assert status == 422, status
                status, body, login_headers = request(host, 'POST', cookie=cookie_header, csrf=probe['csrf'], origin=f'https://{host}:{https}')
                assert status == 200 and json.loads(body)['authenticated'], body[:5000]
                for key, value in login_headers:
                    if key.lower() == 'set-cookie':
                        cookies.load(value)
                session = cookies['session_id']
                assert not session['domain'] and session['secure'] and session['httponly'], session
                cookie_header = '; '.join(f'{key}={morsel.value}' for key, morsel in cookies.items())
                assert json.loads(request(host, cookie=cookie_header)[1])['authenticated']
                record = json.loads(cli(copy, 'status').stdout)['recorded_session']
                backend = http.client.HTTPConnection('127.0.0.1', record['backend_port'], timeout=5)
                backend.request('GET', '/up', headers={'Host': 'foreign.example'})
                response = backend.getresponse()
                assert response.status == 403, response.status
                response.read(); backend.close()
                probes.append((probe, cookie_header))
            # Sending A's cookie deliberately to B still cannot authenticate with B's independent secret.
            assert not json.loads(request('auth-shirabe.localhost', cookie=probes[0][1])[1])['authenticated']
            for copy, environment, _ in copies:
                runtime = state / 'environments' / environment
                assert all((runtime / f'data/development{suffix}.sqlite3').is_file() for suffix in ['', '_dictionary', '_queue', '_cache'])
                assert not any((copy / 'storage').glob('development*.sqlite3'))
                assert not (copy / 'tmp/letter_opener').exists()
            servers[0].send_signal(signal.SIGTERM)
            servers[0].wait(timeout=12)
            assert request('auth-shirabe.localhost', path='/up')[0] == 200
            if args.process_compose:
                copy, environment, _ = copies[0]
                restarted = spawn(copy, 'up', '--readiness-timeout', '90', '--process-compose', args.process_compose)
                wait_for(lambda: request('shirabe.localhost', path='/up')[0] == 200, [proxy, servers[1], restarted], timeout=100)
                compose_processes(args.process_compose, state / 'environments' / environment, root / 'pc-client.log')
                assert json.loads(request('shirabe.localhost', cookie=probes[0][1])[1])['authenticated']
                restarted.send_signal(signal.SIGTERM)
                restarted.wait(timeout=12)
            # Verify originals stayed at the snapshot input bytes; concurrent edits invalidate this evidence.
            for _, _, hashes in copies:
                import hashlib
                for name, digest in hashes.items():
                    assert hashlib.sha256((Path(args.source) / name).read_bytes()).hexdigest() == digest, f'Source changed: {name}'
            print(json.dumps({'verified': ['real Shirabe web boot in two bounded snapshots', 'fresh independent SQLite schema/seed', 'named TLS without trust install',
                'exact host authorization', 'HTTPS CSRF origin checks', 'canonical route/mailer URLs', 'Cable origin configuration',
                'actual auth concern host-only secure cookies', 'cross-environment cookie rejection', 'independent teardown', 'native graph restart with retained data/auth', 'original tracked source unchanged'],
                'native_css_jobs_verified_running': bool(native),
                'dictionary': 'fresh schema only; no imported corpus', 'unverified': ['actual Cable transport in Shirabe', 'SDK watcher', 'full application regression', 'phone access', 'standard ports'],
                'test_ports': {'https': https, 'http': http_port}}))
        except Exception:
            for log in logs:
                log.flush(); log.seek(0)
                print(log.read()[-20000:])
            raise
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    child.send_signal(signal.SIGTERM)
                    try:
                        child.wait(timeout=12)
                    except subprocess.TimeoutExpired:
                        child.kill(); child.wait()
            for log in logs:
                log.close()

if __name__ == '__main__':
    main()
