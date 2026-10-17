"""Named non-HTTP ports across two parallel copies of one project, with real Caddy and Process Compose."""
import argparse
import json
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile

from check_live import BINARY, FIXTURE, free_port, on_terminal, wait_for

LISTENER = FIXTURE.parent / 'port_listener.py'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--caddy', required=True)
    parser.add_argument('--process-compose', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix='pcl-f-ports-', dir='/tmp')).resolve()
    state = root / 'state'
    https, http = free_port(), free_port()
    report = {'complete': False, 'root': str(root), 'front_ports': [https, http], 'cases': [], 'runs': []}
    children, handles, sources, leased = [], [], [], set()

    def cli(path, *argv, check=True, timeout=30):
        return subprocess.run([str(BINARY), '--path', str(path), '--state-dir', str(state), *argv],
                              capture_output=True, text=True, check=check, timeout=timeout)

    def note(name):
        report['cases'].append(name)

    def observe(source, label):
        """Every view of one run's named ports must agree: process argv and env, context.json, context and status."""
        status = json.loads(cli(source, 'status').stdout)
        session = status['recorded_session']
        runtime = state / 'environments' / status['workspace']['id']
        record = wait_for(lambda: json.loads((runtime / 'port-listener.json').read_text()) if (runtime / 'port-listener.json').exists()
                          and json.loads((runtime / 'port-listener.json').read_text())['generation'] == str(session['generation']) else None)
        ports = session['ports']
        assert set(ports) == {'db', 'vite-hmr'}, session
        expected = {'PARCELA_PORT_DB': str(ports['db']), 'PARCELA_PORT_VITE_HMR': str(ports['vite-hmr'])}
        assert record['argv'] == [str(ports['db'])], (record, ports)
        assert record['environment'] == expected, (record, ports)
        environment = json.loads((runtime / 'context.json').read_text())['environment']
        assert {key: environment[key] for key in expected} == expected, environment
        assert environment['DATABASE_URL'] == f"postgres://127.0.0.1:{ports['db']}/app", environment
        assert json.loads(cli(source, 'context').stdout)['ports'] == ports
        human = on_terminal([str(BINARY), '--path', str(source), '--state-dir', str(state), 'status'])
        assert f"{ports['db']}" in human and 'PARCELA_PORT_VITE_HMR' in human, human
        assert 'PARCELA_PORT_DB' in on_terminal([str(BINARY), '--path', str(source), '--state-dir', str(state), 'context'])
        with socket.socket() as probe:
            assert probe.connect_ex(('127.0.0.1', ports['db'])) == 0, 'the stand-in database listens on its leased port'
        endpoints = {endpoint['backend_port'] for endpoint in session['endpoints']} or {session['backend_port']}
        assert not endpoints & set(ports.values()), session
        leased.update(endpoints | set(ports.values()))
        report['runs'].append({'copy': label, 'generation': session['generation'], 'ports': ports, 'endpoints': sorted(endpoints)})
        return ports, endpoints

    try:
        for label in ['alpha', 'beta']:
            source = root / label
            source.mkdir()
            (source / 'parcela.yaml').write_text(f'''project: ports-demo
web: python3 {FIXTURE} web
workers:
  db: python3 {LISTENER} ${{PARCELA_PORT_DB}}
ports: [db, vite-hmr]
environment:
  DATABASE_URL: postgres://127.0.0.1:${{PARCELA_PORT_DB}}/app
tasks:
  ports:
    argv: [python3, -c, 'import json,os;print(json.dumps({{k: v for k, v in os.environ.items() if k.startswith("PARCELA_PORT_")}}))']
    timeout_seconds: 10
''')
            sources.append(source)
        alpha, beta = sources
        assert next(c for c in json.loads(cli(alpha, '--json', 'doctor', check=False).stdout)['checks'] if c['name'] == 'config')['ok']
        proxy = subprocess.Popen([str(BINARY), '--path', str(root), '--state-dir', str(state), 'proxy', 'run', '--caddy', args.caddy,
                                  '--port', str(https)],
                                 stdout=(log := open(root / 'proxy.log', 'w')), stderr=log)
        children.append(proxy)
        handles.append(log)
        wait_for(lambda: cli(root, 'proxy', 'status', check=False).returncode == 0, [proxy])

        cli(alpha, 'up', '--detach', '--process-compose', args.process_compose)
        cli(beta, 'up', '--detach', '--name', 'beta', '--process-compose', args.process_compose)
        first, first_endpoints = observe(alpha, 'alpha')
        other, other_endpoints = observe(beta, 'beta')
        assert not set(first.values()) & set(other.values()), (first, other)
        assert not (set(first.values()) | first_endpoints) & (set(other.values()) | other_endpoints)
        note('parallel-copies-get-distinct-named-ports-that-every-view-agrees-on')

        task = json.loads(cli(alpha, 'task', 'ports').stdout)
        assert task == {'PARCELA_PORT_DB': str(first['db']), 'PARCELA_PORT_VITE_HMR': str(first['vite-hmr'])}, task
        note('task-environment-carries-named-ports')

        cli(alpha, 'down')
        cli(alpha, 'up', '--detach', '--process-compose', args.process_compose)
        assert observe(alpha, 'alpha')[0] == first
        restarted = json.loads(cli(alpha, 'restart', '--detach', '--process-compose', args.process_compose).stdout)
        assert restarted['session']['ports'] == first, restarted['session']
        assert observe(alpha, 'alpha')[0] == first
        note('down-up-and-restart-reuse-the-previous-named-ports')

        cli(alpha, 'down')
        with socket.socket() as foreign:
            foreign.bind(('127.0.0.1', first['db']))
            foreign.listen()
            cli(alpha, 'up', '--detach', '--process-compose', args.process_compose)
            moved, _ = observe(alpha, 'alpha')
            assert moved['db'] != first['db'] and moved['vite-hmr'] == first['vite-hmr'], (moved, first)
        note('a-port-taken-meanwhile-is-replaced-and-the-rest-stay')
        report['complete'] = True
    finally:
        for source in sources:
            try:
                cli(source, 'down', check=False)
            except (subprocess.TimeoutExpired, OSError):
                report['complete'] = False
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
        for handle in handles:
            handle.close()
        report['leased_ports'] = sorted(leased)
        report['listeners_closed'] = True
        for port in [https, http, *leased]:
            with socket.socket() as sock:
                if sock.connect_ex(('127.0.0.1', port)) == 0:
                    report['listeners_closed'] = False
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        if report['complete'] and report['listeners_closed']:
            shutil.rmtree(root)
    assert report['complete'] and report['listeners_closed'], report
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
