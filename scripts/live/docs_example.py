"""The Getting started app from examples/docs-demo in a main checkout and a Git worktree, then cleanup."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from live.common import ROOT, Live, wait_for

SOURCE = ROOT / 'examples/docs-demo'
INPUTS = ['parcela.yaml', 'index.html', 'api.py', 'worker.py']


def digests():
    return {name: hashlib.sha256((SOURCE / name).read_bytes()).hexdigest() for name in INPUTS}


def run(caddy, compose, **_):
    before = digests()
    with Live('docs', caddy, compose) as live:
        primary, signup = live.checkout('hello'), live.root / 'signup'
        live.sources.append(signup)

        def git(*argv):
            subprocess.run(['git', '-C', str(primary), '-c', 'user.name=docs', '-c', 'user.email=docs@example.invalid', *argv],
                           check=True, capture_output=True, timeout=30)

        def remember(result):
            session = result['session']
            assert {endpoint['service'] for endpoint in session['endpoints']} == {'web', 'api'}
            live.ports.extend(endpoint['backend_port'] for endpoint in session['endpoints'])
            return session

        for name in INPUTS:
            shutil.copyfile(SOURCE / name, primary / name)
        git('init', '-q')
        git('add', '.')
        git('commit', '-qm', 'hello app')
        git('worktree', 'add', '-q', str(signup))
        proxy = live.start_proxy()
        sessions, database_urls = [], []
        for path, base in [(primary, 'hello.localhost'), (signup, 'signup-hello.localhost')]:
            sessions.append(remember(live.json(path, 'up', '--detach', '--process-compose', compose)))
            status = live.json(path, 'status')
            assert status['state'] == 'ready' and status['live_health_verified']
            assert all(endpoint['listener_owned'] and endpoint['http_ready'] for endpoint in status['live']['endpoints'].values())
            assert live.cli(path, 'url').stdout.strip() == f'https://{base}:{live.https}'
            assert live.cli(path, 'url', '--service', 'api').stdout.strip() == f'https://api-{base}:{live.https}'
            assert b'Hello from Parcela' in live.request(base, path='/')
            assert json.loads(live.request(f'api-{base}', path='/')) == {'message': 'hello from the API', 'url': f'https://api-{base}:{live.https}'}
            assert {entry['name']: entry['state'] for entry in status['processes']} == {'web': 'running', 'api': 'running', 'ticker': 'running'}
            wait_for(lambda: 'tick 1' in live.cli(path, 'logs', 'ticker', '--tail', '10').stdout, [proxy])
            database_url = live.cli(path, 'task', 'db-url').stdout.strip()
            assert database_url.startswith('postgres://127.0.0.1:') and database_url.endswith('/hello')
            database_urls.append(database_url)
        assert len(set(database_urls)) == 2, database_urls
        listed = live.json(primary, 'ls')['workspaces']
        assert {Path(entry['workspace']['source']).resolve() for entry in listed} >= {primary, signup}
        restarted = remember(live.json(primary, 'restart', '--detach', '--process-compose', compose))
        assert restarted['generation'] == sessions[0]['generation'] + 1 and restarted['token'] != sessions[0]['token']
        assert live.cli(primary, 'task', 'db-url').stdout.strip() == database_urls[0]
        live.cli(primary, 'down')
        assert live.json(primary, 'status')['state'] == 'stopped'
        live.request('hello.localhost', 503, path='/')
        assert b'Hello from Parcela' in live.request('signup-hello.localhost', path='/')
        live.cli(primary, 'down', '--all')
        git('worktree', 'remove', str(signup))
        live.sources.remove(signup)
        live.cli(primary, 'rm', '--prune')
        remaining = {Path(entry['workspace']['source']).resolve() for entry in live.json(primary, 'ls')['workspaces']}
        assert signup not in remaining and primary in remaining, remaining
        assert {name: hashlib.sha256((primary / name).read_bytes()).hexdigest() for name in INPUTS} == before
    assert digests() == before, 'examples/docs-demo is unchanged'
    return {'verified': ['primary and Git worktree environments', 'named HTTPS web and API per checkout',
                         'per-service URL variables', 'worker logs', 'distinct named ports per checkout',
                         'generation restart keeps the named port', 'independent stop and route withdrawal',
                         'down --all, worktree removal and rm --prune', 'source retention']}
