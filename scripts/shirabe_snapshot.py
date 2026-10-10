"""Bounded tracked-source snapshot and explicit opt-in recipe, never edits its source."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
ALLOW_DIRS = {'app', 'bin', 'config', 'db', 'lib', 'public', 'script', 'vendor'}
ALLOW_FILES = {'Gemfile', 'Gemfile.lock', '.ruby-version', 'Rakefile', 'config.ru', 'parcela.yaml'}

def snapshot(source, target):
    source, target = Path(source).resolve(), Path(target).resolve()
    if not (source / '.jj').is_dir():
        raise RuntimeError('This Shirabe snapshot helper requires jj; no Git fallback or source mutation')
    if target.exists():
        raise RuntimeError('Snapshot destination must not already exist')
    tracked = subprocess.check_output(['jj', 'file', 'list'], cwd=source, text=True).splitlines()
    files, total = {}, 0
    for name in tracked:
        relative = Path(name)
        if relative.parts[0] not in ALLOW_DIRS and name not in ALLOW_FILES:
            continue
        if name.startswith('config/credentials') or name == 'config/master.key' or relative.name.startswith('.env'):
            continue
        if any(part in {'storage', 'tmp', 'log', 'node_modules', 'bundle', 'cache'} for part in relative.parts):
            continue
        path = source / relative
        if path.is_symlink():
            raise RuntimeError(f'Refusing tracked symlink in disposable snapshot: {name}')
        data = path.read_bytes()
        total += len(data)
        if len(data) > 16 * 1024 * 1024 or total > 128 * 1024 * 1024:
            raise RuntimeError('Snapshot exceeded explicit bounded-source budget')
        files[name] = (data, path.stat().st_mode & 0o777)
    target.mkdir(mode=0o700)
    hashes = {}
    for name, (data, mode) in files.items():
        destination = target / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        destination.chmod(mode)
        hashes[name] = hashlib.sha256(data).hexdigest()
    for name, digest in hashes.items():
        if hashlib.sha256((source / name).read_bytes()).hexdigest() != digest:
            raise RuntimeError('Source changed during snapshot; acceptance inputs are not stable')
    (target / '.parcela-disposable.json').write_text(json.dumps({'source': str(source), 'file_sha256': hashes, 'bytes': total}))
    install_recipe(target)
    return hashes

def replace_once(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f'Shirabe integration boundary changed; review {path.name} before adapting it')
    path.write_text(text.replace(old, new))

def install_recipe(target):
    if not (target / '.parcela-disposable.json').is_file():
        raise RuntimeError('Integration helper only accepts its own disposable snapshot')
    shutil.copyfile(ROOT / 'examples/shirabe/parcela.yaml', target / 'parcela.yaml')
    integrated = (target / 'config/parcela_development.rb').is_file()
    (target / 'script').mkdir(exist_ok=True)
    shutil.copyfile(ROOT / 'examples/shirabe/parcela_prepare.rb', target / 'script/parcela_prepare.rb')
    if not integrated:
        shutil.copyfile(ROOT / 'examples/shirabe/config/parcela_development.rb', target / 'config/parcela_development.rb')
        development = target / 'config/environments/development.rb'
        old = '''  config.middleware.insert_before 0, Rack::Config do |env|
        if env["HTTP_X_FORWARDED_PROTO"] == "https"
          env["HTTPS"] = "on"
          env["rack.url_scheme"] = "https"
        end
      end'''
        replace_once(development, old, '')
        with development.open('a') as file:
            file.write('\nrequire_relative "../parcela_development" if ENV["PARCELA_URL"]\n')
        database = target / 'config/database.yml'
        for suffix in ['', '_dictionary', '_queue', '_cache']:
            replace_once(database, f'database: storage/development{suffix}.sqlite3',
                         f'database: <%= File.join(ENV.fetch("SHIRABE_DATABASE_DIRECTORY", "storage"), "development{suffix}.sqlite3").inspect %>')
        storage = target / 'config/storage.yml'
        replace_once(storage, ': Rails.root.join("storage")', ': Pathname.new(ENV.fetch("SHIRABE_UPLOADS_DIRECTORY", Rails.root.join("storage").to_s))')
        authentication = target / 'app/controllers/concerns/authentication.rb'
        replace_once(authentication, 'same_site: :lax, domain: :all', 'same_site: :lax, secure: ENV["SHIRABE_PREVIEW"] == "1", domain: (ENV["SHIRABE_PREVIEW"] == "1" ? nil : :all)')
        deletion = 'cookies.delete(:session_id, domain: :all)'
        text = authentication.read_text()
        if text.count(deletion) != 2:
            raise RuntimeError('Authentication deletion boundaries changed; review source')
        authentication.write_text(text.replace(deletion, 'cookies.delete(:session_id, domain: (ENV["SHIRABE_PREVIEW"] == "1" ? nil : :all))'))
        replace_once(target / 'Gemfile', '  gem "rb-portless", "0.5.2", require: "portless/rails"\n', '')
    for name in ['tmp', 'log', 'storage']:
        (target / name).mkdir(exist_ok=True)
    # Every writable source-relative path is inside this disposable copy. No symlink into user data.
    (target / 'tmp/pids').mkdir(exist_ok=True)
