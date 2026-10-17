"""Multi-service graphs: one route group per checkout, per-endpoint health, recovery and partial-start failure.

Lifecycle commands target workspaces by UUID, as the operating skill tells agents to.
"""
import json
import os
from pathlib import Path
import signal
import socket
import subprocess

from live.common import BINARY, SERVICE, Live, wait_for

MANIFEST = f'''version: 1
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
      command: python3 {SERVICE} frontend
    backend:
      working_dir: backend
      command: python3 {SERVICE} backend
      environment:
        - PORT=${{PARCELA_SERVICE_BACKEND_PORT}}
    worker:
      command: python3 -c 'import time; print("service-set-worker", flush=True); time.sleep(600)'
'''
LEGACY = f'''version: 1
project: {{id: service-sets, name: sets}}
default_service: web
services: {{web: {{readiness_path: /ready}}}}
process_compose:
  version: '0.5'
  processes:
    server:
      command: python3 {SERVICE}
'''
BY_UUID = {'status', 'url', 'logs', 'restart', 'recover', 'down', 'task'}


def run(caddy, compose, **_):
    with Live('sets', caddy, compose) as live:
        workspaces, cases = {}, []

        def cli(path, *argv, check=True, timeout=20):
            if argv[0] in BY_UUID and path in workspaces:
                return subprocess.run([str(BINARY), '--workspace', workspaces[path]['id'], '--state-dir', str(live.state), *argv],
                                      capture_output=True, text=True, check=check, timeout=timeout)
            return live.cli(path, *argv, check=check, timeout=timeout)

        def status(path):
            return json.loads(cli(path, 'status').stdout)

        def remember(session):
            assert len(session['endpoints']) == 2
            live.ports.extend(endpoint['backend_port'] for endpoint in session['endpoints'])
            return session['endpoints']

        request = live.request
        for label, role in [('primary', 'primary'), ('review', 'preview')]:
            source = live.checkout(label, MANIFEST)
            for name in ['frontend', 'backend']:
                (source / name).mkdir()
            workspaces[source] = live.register(source, role, label)
        primary, preview = workspaces
        workspace = workspaces[primary]
        doctor = json.loads(cli(primary, '--json', 'doctor', check=False).stdout)
        assert next(check for check in doctor['checks'] if check['name'] == 'config')['ok']
        context = json.loads(cli(primary, 'context').stdout)
        assert context['workspace']['id'] == workspace['id'] and context['workspace']['role'] == 'primary'
        assert set(context['planned_urls']) == {'frontend', 'backend'}
        inventory = json.loads(cli(live.root, 'ls').stdout)['workspaces']
        assert {row['workspace']['id'] for row in inventory} == {entry['id'] for entry in workspaces.values()}
        assert status(primary)['state'] == 'stopped'
        proxy = live.start_proxy()
        runtime = live.state / 'environments' / workspace['id']
        for directory in (runtime.parent, runtime):
            directory.mkdir(mode=0o700, exist_ok=True)
        gate = runtime / 'unhealthy-backend'
        gate.touch()
        launcher = live.spawn(primary, 'up', '--detach', '--process-compose', compose)

        def waiting_for_api():
            observed = status(primary)
            return observed if (observed.get('live') or {}).get('endpoints', {}).get('frontend', {}).get('http_ready') else None
        starting = wait_for(waiting_for_api, [proxy, launcher])
        assert starting['state'] == 'starting' and not starting['live']['endpoints']['backend']['http_ready'], starting
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        gate.unlink()
        assert launcher.wait(timeout=20) == 0
        first = json.loads(Path(live.logs[-1].name).read_text())
        cases.append('no-route-before-all-required-endpoints-ready')
        endpoints = remember(first['session'])
        assert first['session']['generation'] == 2
        for host, service in [('sets.localhost', 'frontend'), ('api-sets.localhost', 'backend')]:
            body = request(host)
            assert body['id'] == workspace['id'] and body['argv'] == [service], body
            assert body['cwd'] == str(primary / service), body
            assert body['api_origin'] == f'https://api-sets.localhost:{live.https}', body
            assert body['headers']['X-Forwarded-Host'] == host, body
            assert body['headers']['X-Parcela-Generation'] == '2', body
        assert len({endpoint['backend_port'] for endpoint in endpoints}) == 2
        current = status(primary)
        assert current['live']['state'] == 'ready' and set(current['live']['endpoints']) == {'frontend', 'backend'}, current
        assert all(endpoint['http_ready'] and endpoint['listener_owned'] for endpoint in current['live']['endpoints'].values()), current
        assert {entry['name']: entry['state'] for entry in current['processes']} == {'frontend': 'running', 'backend': 'running', 'worker': 'running'}
        assert cli(primary, 'url', '--service', 'backend').stdout.strip() == f'https://api-sets.localhost:{live.https}'
        cases.append('two-origins-one-generation-working-directories-and-worker')
        wait_for(lambda: 'service-set-worker' in cli(primary, 'logs', 'worker', '--tail', '10').stdout, [proxy])
        assert json.loads(cli(primary, 'task', 'probe', '--', 'literal argument with spaces').stdout) == ['literal argument with spaces']
        cases.append('agent-context-inventory-uuid-health-logs-and-explicit-task')

        second = json.loads(cli(preview, 'up', '--detach', '--process-compose', compose).stdout)
        remember(second['session'])
        assert request('review-sets.localhost')['id'] != workspace['id']
        assert request('api-review-sets.localhost')['id'] != workspace['id']
        cases.append('independent-worktree-service-set')
        collision = live.checkout('collision', MANIFEST)
        refused = cli(collision, 'up', '--name', 'api', '--process-compose', '/nonexistent/parcela-register-only', check=False)
        assert refused.returncode == 1 and 'is already used by' in refused.stderr, refused
        assert request('api-sets.localhost')['id'] == workspace['id']
        cases.append('secondary-host-cannot-be-another-worktree-base')
        unhealthy = runtime / 'unhealthy-backend'
        unhealthy.touch()
        current = status(primary)['live']
        assert current['state'] == 'unhealthy' and not current['endpoints']['backend']['http_ready'], current
        assert current['endpoints']['frontend']['http_ready'], current
        unhealthy.unlink()
        cases.append('per-endpoint-health')
        frontend_pid = request('sets.localhost')['pid']
        unhealthy.touch()
        subprocess.run([compose, '--use-uds', '--unix-socket', json.loads((runtime / 'context.json').read_text())['sockets']['compose'],
                        '--token-file', str(runtime / 'pc.token'), '--log-file', str(runtime / 'log/probe.log'),
                        'process', 'restart', 'backend'], check=True, capture_output=True, timeout=10)
        wait_for(lambda: status(primary)['state'] == 'starting', [proxy])
        wait_for(lambda: request('sets.localhost', 503), [proxy])
        request('api-sets.localhost', 503)
        unhealthy.unlink()
        wait_for(lambda: status(primary)['state'] == 'ready', [proxy])
        assert request('sets.localhost')['pid'] == frontend_pid
        cases.append('changed-secondary-listener-withdraws-and-reestablishes-group')
        before_pid = request('api-sets.localhost')['pid']
        owner = status(primary)['live']['owner_pid']
        assert owner == first['live']['owner_pid']
        (primary / 'parcela.yaml').write_text(MANIFEST.replace('subdomain: api', 'subdomain: future-api'))
        os.kill(owner, signal.SIGKILL)
        wait_for(lambda: request('api-sets.localhost', 503), [proxy])
        wait_for(lambda: request('sets.localhost', 503), [proxy])
        recovered = json.loads(cli(primary, 'recover', '--detach').stdout)
        assert recovered['session']['token'] == first['session']['token']
        assert request('api-sets.localhost')['pid'] == before_pid
        assert request('sets.localhost')['generation'] == '2'
        assert request('api-sets.localhost')['api_origin'] == f'https://api-sets.localhost:{live.https}'
        (primary / 'parcela.yaml').write_text(MANIFEST)
        cases.append('group-owner-death-and-authenticated-recovery-without-replay')
        with socket.socket() as foreign:
            foreign.bind(('127.0.0.1', 0))
            foreign.listen()
            refused = cli(primary, 'restart', '--detach', '--process-compose', compose,
                          '--backend-port', str(foreign.getsockname()[1]), check=False)
            assert refused.returncode != 0
            assert request('api-sets.localhost')['pid'] == before_pid
        restarted = json.loads(cli(primary, 'restart', '--detach', '--process-compose', compose).stdout)
        remember(restarted['session'])
        assert restarted['session']['generation'] == 3 and restarted['session']['token'] != first['session']['token']
        assert request('api-sets.localhost')['pid'] != before_pid
        assert request('sets.localhost')['generation'] == '3'
        cases.append('whole-graph-restart-and-foreign-endpoint-refusal')
        (runtime / 'data' / 'retain.txt').write_text('retained')
        cli(primary, 'down')
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        assert request('api-review-sets.localhost')['generation'] == '2'
        assert (runtime / 'data' / 'retain.txt').read_text() == 'retained'
        cli(primary, 'down')
        cases.append('group-stop-is-idempotent-and-preserves-other-worktree-and-data')
        (primary / 'parcela.yaml').write_text(MANIFEST.replace(f'python3 {SERVICE} backend', "python3 -c 'import time; time.sleep(600)'"))
        failed = cli(primary, 'up', '--detach', '--process-compose', compose, '--readiness-timeout', '2', check=False)
        assert failed.returncode != 0
        request('sets.localhost', 503)
        request('api-sets.localhost', 503)
        final = status(primary)['recorded_session']
        remember(final)
        assert final['status'] == 'stopped', final
        cases.append('partial-startup-failure-never-publishes-and-stops-group')
        legacy = live.checkout('legacy', LEGACY)
        workspaces[legacy] = live.register(legacy, 'preview', 'legacy')
        single = json.loads(cli(legacy, 'up', '--detach', '--process-compose', compose).stdout)
        live.ports.append(single['session']['backend_port'])
        assert request('legacy-sets.localhost')['id'] == workspaces[legacy]['id']
        assert json.loads(cli(legacy, 'status').stdout)['processes'][0]['name'] == 'server'
        cases.append('legacy-single-service-allows-a-different-native-process-name')
        return {'https_port': live.https, 'cases': cases}
