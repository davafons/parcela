"""Real Caddy/Process Compose service-set acceptance in isolated state."""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import ssl
import subprocess
import tempfile

from check_live import BINARY, FIXTURE, free_port, wait_for


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--agent-recipe', action='store_true', help='Exercise the operating skill workflow, including UUID targets and explicit tasks')
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix='pc-set-', dir='/tmp')).resolve()
    state = root / 'state'
    https, http = free_port(), free_port()
    report = {'complete': False, 'agent_recipe': args.agent_recipe, 'root': str(root), 'front_ports': [https, http], 'cases': []}
    children, handles, sources, ports = [], [], [], []

    def cli(path, *argv, check=True, timeout=20):
        target = ['--path', str(path)]
        if args.agent_recipe and argv and argv[0] in {'status', 'services', 'url', 'logs', 'restart', 'recover', 'down', 'task'}:
            workspace = next((workspace for source, workspace in sources if source == path), None)
            if workspace:
                target = ['--workspace', workspace['id']]
        return subprocess.run([str(BINARY), *target, '--state-dir', str(state), *argv],
                              capture_output=True, text=True, check=check, timeout=timeout)

    def spawn(path, *argv):
        log = open(root / f'process-{len(children)}.log', 'w')
        handles.append(log)
        child = subprocess.Popen([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv], stdout=log, stderr=log)
        children.append(child)
        return child

    def request(host, expected=200):
        context = ssl.create_default_context(cafile=str(state / 'proxy/data/pki/authorities/local/root.crt'))
        with context.wrap_socket(socket.create_connection(('127.0.0.1', https), timeout=3), server_hostname=host) as sock:
            sock.sendall(f'GET /ready HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n'.encode())
            data = b''
            while part := sock.recv(8192):
                data += part
        head, body = data.split(b'\r\n\r\n', 1)
        if not head.startswith(f'HTTP/1.1 {expected}'.encode()):
            raise ValueError(f'expected HTTP {expected}, received {head!r}')
        return json.loads(body) if expected == 200 else body

    def status(path):
        return json.loads(cli(path, 'status').stdout)

    def note(name):
        report['cases'].append(name)

    def remember(session):
        endpoints = session['endpoints']
        assert len(endpoints) == 2
        ports.extend(endpoint['backend_port'] for endpoint in endpoints)
        return endpoints

    try:
        for label, role in [('primary', 'primary'), ('review', 'preview')]:
            source = root / label
            source.mkdir()
            for name in ['frontend', 'backend']:
                (source / name).mkdir()
            (source / 'parcela.yaml').write_text(f'''version: 1
project: {{id: service-sets, name: sets}}
default_service: frontend
services:
  frontend: {{readiness_path: /ready}}
  backend: {{subdomain: api, readiness_path: /ready}}
tasks:
  probe:
    argv: [python3, -c, 'import json,sys;print(json.dumps(sys.argv[1:]))']
    timeout_seconds: 5
environment:
  API_ORIGIN: '${{PARCELA_SERVICE_BACKEND_URL}}'
process_compose:
  version: '0.5'
  processes:
    frontend:
      working_dir: frontend
      command: python3 {FIXTURE} frontend
    backend:
      working_dir: backend
      command: python3 {FIXTURE} backend
      environment:
        - PORT=${{PARCELA_SERVICE_BACKEND_PORT}}
    worker:
      command: python3 -c 'import time; print("service-set-worker", flush=True); time.sleep(600)'
''')
            workspace = json.loads(cli(source, 'workspace', 'adopt', '--role', role, '--label', label).stdout)['workspace']
            sources.append((source, workspace))
        primary, workspace = sources[0]
        preview, _ = sources[1]
        if args.agent_recipe:
            assert json.loads(cli(primary, 'config', 'check').stdout)['valid']
            context = json.loads(cli(primary, 'context').stdout)
            assert context['workspace']['id'] == workspace['id'] and context['workspace']['role'] == 'primary'
            assert set(context['planned_urls']) == {'frontend', 'backend'}
            inventory = json.loads(cli(root, 'workspace', 'list', '--all').stdout)['workspaces']
            assert {row['workspace']['id'] for row in inventory} == {entry['id'] for _, entry in sources}
            assert status(primary)['state'] == 'stopped'
        proxy = spawn(root, 'proxy', 'run', '--caddy', args.caddy, '--test-profile', '--https-port', str(https), '--http-port', str(http))
        wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])
        environments = state / 'environments'
        environments.mkdir(mode=0o700, exist_ok=True)
        runtime = environments / workspace['id']
        runtime.mkdir(mode=0o700, exist_ok=True)
        gate = runtime / 'unhealthy-backend'
        gate.touch()
        launcher = spawn(primary, 'up', '--detach', '--process-compose', args.process_compose)
        def waiting_for_api():
            observed = status(primary)
            return observed if (observed.get('live') or {}).get('endpoints', {}).get('frontend', {}).get('http_ready') else None
        starting = wait_for(waiting_for_api, [proxy, launcher])
        assert starting['state'] == 'starting' and not starting['live']['endpoints']['backend']['http_ready'], starting
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        gate.unlink()
        assert launcher.wait(timeout=20) == 0
        first = json.loads((root / 'process-1.log').read_text())
        note('no-route-before-all-required-endpoints-ready')
        endpoints = remember(first['session'])
        assert first['session']['generation'] == 2
        for host, service, directory in [('sets.localhost', 'frontend', 'frontend'), ('api-sets.localhost', 'backend', 'backend')]:
            body = request(host)
            assert body['id'] == workspace['id'] and body['argv'] == [service], body
            assert body['cwd'] == str(primary / directory), body
            assert body['api_origin'] == f'https://api-sets.localhost:{https}', body
            assert body['headers']['X-Forwarded-Host'] == host, body
            assert body['headers']['X-Parcela-Generation'] == '2', body
        assert len({endpoint['backend_port'] for endpoint in endpoints}) == 2
        live = status(primary)['live']
        assert live['state'] == 'ready' and set(live['endpoints']) == {'frontend', 'backend'}, live
        assert all(endpoint['http_ready'] and endpoint['listener_owned'] for endpoint in live['endpoints'].values()), live
        services = json.loads(cli(primary, 'services').stdout)['services']
        assert {entry['name']: entry['status'] for entry in services} == {'frontend': 'Running', 'backend': 'Running', 'worker': 'Running'}, services
        assert cli(primary, 'url', '--service', 'backend').stdout.strip() == f'https://api-sets.localhost:{https}'
        note('two-origins-one-generation-working-directories-and-worker')
        if args.agent_recipe:
            wait_for(lambda: 'service-set-worker' in cli(primary, 'logs', 'worker', '--tail', '10').stdout, [proxy])
            assert json.loads(cli(primary, 'task', 'probe', '--', 'literal argument with spaces').stdout) == ['literal argument with spaces']
            note('agent-context-inventory-uuid-health-logs-and-explicit-task')

        second = json.loads(cli(preview, 'up', '--detach', '--process-compose', args.process_compose).stdout)
        remember(second['session'])
        assert request('review-sets.localhost')['id'] != workspace['id']
        assert request('api-review-sets.localhost')['id'] != workspace['id']
        note('independent-worktree-service-set')
        collision = root / 'collision'
        collision.mkdir()
        (collision / 'parcela.yaml').write_text((primary / 'parcela.yaml').read_text())
        refused = cli(collision, 'workspace', 'adopt', '--role', 'preview', '--label', 'api', check=False)
        assert refused.returncode != 0, refused
        assert request('api-sets.localhost')['id'] == workspace['id']
        note('secondary-host-cannot-be-another-worktree-base')
        unhealthy = state / 'environments' / workspace['id'] / 'unhealthy-backend'
        unhealthy.touch()
        live = status(primary)['live']
        assert live['state'] == 'unhealthy' and not live['endpoints']['backend']['http_ready'], live
        assert live['endpoints']['frontend']['http_ready'], live
        unhealthy.unlink()
        note('per-endpoint-health')
        frontend_pid = request('sets.localhost')['pid']
        unhealthy.touch()
        subprocess.run([args.process_compose, '--use-uds', '--unix-socket', json.loads((runtime / 'context.json').read_text())['sockets']['compose'],
                        '--token-file', str(runtime / 'pc.token'), '--log-file', str(runtime / 'log/probe.log'),
                        'process', 'restart', 'backend'], check=True, capture_output=True, timeout=10)
        wait_for(lambda: status(primary)['state'] == 'starting', [proxy])
        wait_for(lambda: request('sets.localhost', 503), [proxy])
        request('api-sets.localhost', 503)
        unhealthy.unlink()
        wait_for(lambda: status(primary)['state'] == 'ready', [proxy])
        assert request('sets.localhost')['pid'] == frontend_pid
        note('changed-secondary-listener-withdraws-and-reestablishes-group')
        before_pid = request('api-sets.localhost')['pid']
        owner = status(primary)['live']['owner_pid']
        assert owner == first['live']['owner_pid']
        original_manifest = (primary / 'parcela.yaml').read_text()
        (primary / 'parcela.yaml').write_text(original_manifest.replace('subdomain: api', 'subdomain: future-api'))
        os.kill(owner, signal.SIGKILL)
        wait_for(lambda: request('api-sets.localhost', 503), [proxy])
        wait_for(lambda: request('sets.localhost', 503), [proxy])
        recovered = json.loads(cli(primary, 'recover', '--detach').stdout)
        assert recovered['session']['token'] == first['session']['token']
        assert request('api-sets.localhost')['pid'] == before_pid
        assert request('sets.localhost')['generation'] == '2'
        assert request('api-sets.localhost')['api_origin'] == f'https://api-sets.localhost:{https}'
        (primary / 'parcela.yaml').write_text(original_manifest)
        note('group-owner-death-and-authenticated-recovery-without-replay')
        with socket.socket() as foreign:
            foreign.bind(('127.0.0.1', 0))
            foreign.listen()
            refused = cli(primary, 'restart', '--detach', '--process-compose', args.process_compose,
                          '--backend-port', str(foreign.getsockname()[1]), check=False)
            assert refused.returncode != 0
            assert request('api-sets.localhost')['pid'] == before_pid
        restarted = json.loads(cli(primary, 'restart', '--detach', '--process-compose', args.process_compose).stdout)
        remember(restarted['session'])
        assert restarted['session']['generation'] == 3 and restarted['session']['token'] != first['session']['token']
        assert request('api-sets.localhost')['pid'] != before_pid
        assert request('sets.localhost')['generation'] == '3'
        note('whole-graph-restart-and-foreign-endpoint-refusal')
        runtime = state / 'environments' / workspace['id']
        (runtime / 'data' / 'retain.txt').write_text('retained')
        cli(primary, 'down')
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        assert request('api-review-sets.localhost')['generation'] == '2'
        assert (runtime / 'data' / 'retain.txt').read_text() == 'retained'
        cli(primary, 'down')
        note('group-stop-is-idempotent-and-preserves-other-worktree-and-data')
        text = (primary / 'parcela.yaml').read_text()
        (primary / 'parcela.yaml').write_text(text.replace(f'python3 {FIXTURE} backend', "python3 -c 'import time; time.sleep(600)'"))
        failed = cli(primary, 'up', '--detach', '--process-compose', args.process_compose,
                     '--readiness-timeout', '2', check=False)
        assert failed.returncode != 0
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        final = status(primary)['recorded_session']
        remember(final)
        assert final['status'] == 'stopped', final
        note('partial-startup-failure-never-publishes-and-stops-group')
        legacy = root / 'legacy'
        legacy.mkdir()
        (legacy / 'parcela.yaml').write_text(f'''version: 1
project: {{id: service-sets, name: sets}}
default_service: web
services: {{web: {{readiness_path: /ready}}}}
process_compose:
  version: '0.5'
  processes:
    server:
      command: python3 {FIXTURE}
''')
        legacy_workspace = json.loads(cli(legacy, 'workspace', 'adopt', '--role', 'preview', '--label', 'legacy').stdout)['workspace']
        sources.append((legacy, legacy_workspace))
        single = json.loads(cli(legacy, 'up', '--detach', '--process-compose', args.process_compose).stdout)
        ports.append(single['session']['backend_port'])
        assert request('legacy-sets.localhost')['id'] == legacy_workspace['id']
        assert json.loads(cli(legacy, 'services').stdout)['services'][0]['name'] == 'server'
        note('legacy-single-service-allows-a-different-native-process-name')
        report['complete'] = True
    finally:
        for source, _ in sources:
            try:
                cli(source, 'down', check=False, timeout=30)
            except (subprocess.TimeoutExpired, OSError):
                report['complete'] = False
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
        for handle in handles:
            handle.close()
        report['backend_ports'] = sorted(set(ports))
        report['listeners_closed'] = True
        for port in [https, http, *ports]:
            with socket.socket() as sock:
                if sock.connect_ex(('127.0.0.1', port)) == 0:
                    report['listeners_closed'] = False
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        assert report['listeners_closed'], report
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
