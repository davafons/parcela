#!/usr/bin/env python3
"""Explicit operator-only macOS loopback forwarding and CA trust; never called by run/up."""
import argparse
import hashlib
import json
import os
import plistlib
from pathlib import Path
import re
import socket
import ssl
import stat
import subprocess
import sys
import tempfile

ANCHOR = 'com.apple/parcela'
RULE_FILE = Path('/etc/pf.anchors/com.apple.parcela')
TOKEN_FILE = Path('/var/run/parcela-pf.token')
RULES = ('rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port 8080\n'
         'rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 443 -> 127.0.0.1 port 8443\n')
DEFAULT_RULES = [
    'scrub-anchor "com.apple/*"', 'nat-anchor "com.apple/*"', 'rdr-anchor "com.apple/*"',
    'dummynet-anchor "com.apple/*"', 'anchor "com.apple/*"',
    'load anchor "com.apple" from "/etc/pf.anchors/com.apple"',
]

def command(*argv):
    result = subprocess.run(argv, check=True, capture_output=True, text=True, timeout=20)
    return result.stdout, result.stderr

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def protected(path, uid):
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and metadata.st_uid == uid and not metadata.st_mode & 0o022,
            f'Unexpected ownership/type/permissions: {path}')

def may_initialize_root(nat, filters, info, config):
    lines = [line.strip() for line in config.splitlines() if line.strip() and not line.lstrip().startswith('#')]
    return not nat.strip() and not filters.strip() and 'Status: Disabled' in info and lines == DEFAULT_RULES

def tls_probe(port, hostname, certificate):
    context = ssl.create_default_context(cafile=str(certificate))
    with socket.create_connection(('127.0.0.1', port), timeout=5) as raw:
        with context.wrap_socket(raw, server_hostname=hostname) as connection:
            connection.sendall(f'GET /up HTTP/1.1\r\nHost: {hostname}\r\nConnection: close\r\n\r\n'.encode())
            require(connection.recv(1024).startswith(b'HTTP/1.1 '), 'TLS front door did not answer HTTP')

def configure_pf():
    for port in [80, 443]:
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', port))
    nat, _ = command('/sbin/pfctl', '-sn')
    filters, _ = command('/sbin/pfctl', '-sr')
    info, _ = command('/sbin/pfctl', '-s', 'info')
    initialize = may_initialize_root(nat, filters, info, Path('/etc/pf.conf').read_text())
    require('rdr-anchor "com.apple/*"' in nat or initialize,
            'No existing com.apple rdr anchor; refusing to replace a nonempty/custom root ruleset')
    previous, _ = command('/sbin/pfctl', '-a', ANCHOR, '-sn')
    if RULE_FILE.exists() or RULE_FILE.is_symlink():
        protected(RULE_FILE, 0)
        require(RULE_FILE.read_text() == RULES, 'Existing anchor file is not this Parcela configuration')
    else:
        require(not previous.strip(), 'Existing unowned Parcela anchor; refusing takeover')
    if TOKEN_FILE.exists() or TOKEN_FILE.is_symlink():
        protected(TOKEN_FILE, 0)
        require('Status: Enabled' in info, 'Stored PF reference is stale; explicit reconciliation required')
    if not RULE_FILE.exists():
        descriptor = os.open(RULE_FILE, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(descriptor, 'w') as file:
            file.write(RULES)
    command('/sbin/pfctl', '-n', '-a', ANCHOR, '-f', str(RULE_FILE))
    if initialize:
        command('/sbin/pfctl', '-f', '/etc/pf.conf')
    command('/sbin/pfctl', '-a', ANCHOR, '-f', str(RULE_FILE))
    if not TOKEN_FILE.exists():
        stdout, stderr = command('/sbin/pfctl', '-E')
        token = re.search(r'Token\s*:\s*(\d+)', stdout + stderr)
        require(token is not None, 'PF enabled but reference token unavailable; inspect before retrying')
        descriptor = os.open(TOKEN_FILE, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(descriptor, 'w') as file:
            file.write(token.group(1))

def install_boot_job():
    helper = Path('/Library/PrivilegedHelperTools/org.parcela.pf.py')
    helper.parent.mkdir(mode=0o755, exist_ok=True)
    require(not helper.parent.is_symlink() and helper.parent.stat().st_uid == 0 and not helper.parent.stat().st_mode & 0o022, 'Unsafe privileged helper directory')
    daemon = Path('/Library/LaunchDaemons/org.parcela.pf.plist')
    data = Path(__file__).read_bytes()
    plist = plistlib.dumps({'Label': 'org.parcela.pf', 'ProgramArguments': ['/usr/bin/python3', '-I', str(helper), '--forward-only'],
                           'RunAtLoad': True, 'StandardOutPath': '/var/log/parcela-pf.log', 'StandardErrorPath': '/var/log/parcela-pf.log'})
    for path, content, mode in [(helper, data, 0o755), (daemon, plist, 0o644)]:
        if path.exists() or path.is_symlink():
            protected(path, 0)
            require(path.read_bytes() == content, f'Existing {path} differs; explicit update/reconciliation required')
        else:
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, mode)
            with os.fdopen(descriptor, 'wb') as file:
                file.write(content)
    present = subprocess.run(['/bin/launchctl', 'print', 'system/org.parcela.pf'], capture_output=True, timeout=5)
    if present.returncode:
        command('/bin/launchctl', 'bootstrap', 'system', str(daemon))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state-dir', type=Path)
    parser.add_argument('--hostname')
    parser.add_argument('--forward-only', action='store_true')
    args = parser.parse_args()
    require(sys.platform == 'darwin' and os.geteuid() == 0, 'Run this explicit helper with sudo on macOS')
    if args.forward_only:
        configure_pf()
        return
    require(args.state_dir is not None and args.hostname is not None, '--state-dir and --hostname are required for setup/trust')
    uid = int(os.environ.get('SUDO_UID', '-1'))
    require(uid > 0, 'Run through sudo as the intended unprivileged Parcela user')
    require(re.fullmatch(r'[a-z0-9][a-z0-9-]*\.localhost', args.hostname), 'Expected an explicit flat localhost hostname')
    state = args.state_dir.resolve(strict=True)
    require(state.stat().st_uid == uid and not state.stat().st_mode & 0o077, 'State must remain private and user-owned')
    record = state / 'proxy/router.json'
    protected(record, uid)
    router = json.loads(record.read_text())
    require(router.get('front_door') == {'mode': 'standard', 'https': 8443, 'http': 8080}, 'Expected the explicit 8080/8443 forwarding profile')
    certificate = state / 'proxy/data/pki/authorities/local/root.crt'
    protected(certificate, uid)
    tls_probe(8443, args.hostname, certificate)
    configure_pf()
    install_boot_job()
    tls_probe(443, args.hostname, certificate)
    with tempfile.NamedTemporaryFile(prefix='parcela-ca-', suffix='.crt') as copy:
        data = certificate.read_bytes()
        copy.write(data); copy.flush()
        command('/usr/bin/security', 'add-trusted-cert', '-d', '-r', 'trustRoot', '-k',
                '/Library/Keychains/System.keychain', copy.name)
    command('/usr/bin/curl', '-q', '--noproxy', '*', '--silent', '--show-error', '--max-time', '5', '--resolve',
            f'{args.hostname}:443:127.0.0.1', '--output', '/dev/null', f'https://{args.hostname}/up')
    print(json.dumps({'standard_tls_verified': True, 'system_trust_verified': True, 'trusted_ca_pem_sha256': hashlib.sha256(data).hexdigest(),
                      'ca': str(certificate), 'anchor': ANCHOR, 'pf_reference': str(TOKEN_FILE), 'boot_forwarding_job': 'org.parcela.pf'}))

if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(f'parcela setup: {error}. Setup may be partial; no automatic rollback of host state.', file=sys.stderr)
        sys.exit(1)
