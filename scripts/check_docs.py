"""Check Starlight artifacts, public sources and root/project-subpath HTTP serving."""
import argparse
from functools import partial
import gzip
from html.parser import HTMLParser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import socket
import shutil
import subprocess
import tempfile
import threading
from urllib.parse import unquote, urljoin, urlsplit
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parent.parent
WEBSITE = ROOT / 'docs/website'


class Page(HTMLParser):
    def __init__(self, text):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.references = []
        self.headings = 0
        self.feed(text)

    def handle_starttag(self, tag, attributes):
        attributes = dict(attributes)
        if 'id' in attributes:
            identifier = attributes['id']
            if identifier in self.ids:
                raise ValueError(f'duplicate HTML ID: {identifier}')
            self.ids.add(identifier)
        if tag == 'h1':
            self.headings += 1
        for attribute in ['href', 'src']:
            if attribute in attributes and attributes.get('rel') != 'canonical':
                self.references.append((tag, attribute, attributes[attribute]))


def target_file(site, current, reference, base='/'):
    site, current = site.resolve(), current.resolve()
    parsed = urlsplit(reference)
    if parsed.scheme or parsed.netloc:
        if parsed.scheme not in {'https', 'http', 'mailto'}:
            raise ValueError(f'unsupported link scheme: {reference}')
        return None
    path = unquote(parsed.path)
    if path.startswith('/'):
        if not path.startswith(base):
            raise ValueError(f'link leaves project prefix: {reference}')
        target = (site / path[len(base):]).resolve()
    else:
        target = (current.parent / path).resolve() if path else current
    if not target.is_relative_to(site):
        raise ValueError(f'link escapes site: {reference}')
    if target.is_dir():
        target = target / 'index.html'
    if not target.is_file():
        raise ValueError(f'missing link target: {reference} from {current.relative_to(site)}')
    return target


def contrast_ratio(foreground, background):
    def luminance(color):
        channels = [int(color[offset:offset + 2], 16) / 255 for offset in (1, 3, 5)]
        linear = [value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4
                  for value in channels]
        return sum(value * weight for value, weight in zip(linear, (0.2126, 0.7152, 0.0722)))

    light, dark = sorted((luminance(foreground), luminance(background)), reverse=True)
    return (light + 0.05) / (dark + 0.05)


def check_theme_colors():
    text = (WEBSITE / 'src/parcela.css').read_text()
    palettes = {mode or 'dark': dict(re.findall(r'(--sl-[\w-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;', body))
                for mode, body in re.findall(r':root(?:\[data-theme="(light)"\])?\s*\{([^}]+)\}', text)}
    if set(palettes) != {'dark', 'light'}:
        raise ValueError('expected explicit dark and light palettes')
    pairs = [
        ('body', 'text', 'bg', 4.5),
        ('heading', 'white', 'bg', 4.5),
        ('sidebar label', 'gray-3', 'bg-sidebar', 4.5),
        ('link', 'text-accent', 'bg', 4.5),
        ('selected navigation', 'text-accent', 'accent-low', 4.5),
        ('inline code', 'text', 'bg-inline-code', 4.5),
        ('accent control', 'text-invert', 'text-accent', 4.5),
        ('focus on page', 'text-accent', 'bg', 3),
        ('focus in header', 'text-accent', 'bg-nav', 3),
    ]
    results = {}
    for mode, colors in palettes.items():
        results[mode] = {}
        for label, foreground, background, minimum in pairs:
            ratio = contrast_ratio(colors[f'--sl-color-{foreground}'], colors[f'--sl-color-{background}'])
            if ratio < minimum:
                raise ValueError(f'{mode} {label} contrast {ratio:.2f} is below {minimum}')
            results[mode][label] = round(ratio, 2)
    return results


def check_search(site, base):
    expected = json.loads((site / 'documentation-pages.json').read_text())
    indexed = {}
    for path in (site / 'pagefind/fragment').glob('*.pf_fragment'):
        payload = gzip.decompress(path.read_bytes())
        if not payload.startswith(b'pagefind_dcd'):
            raise ValueError('unsupported Pagefind fragment format')
        entry = json.loads(payload.removeprefix(b'pagefind_dcd'))
        url = base + entry['url'].lstrip('/')
        if url in indexed or not entry['content']:
            raise ValueError(f'duplicate or empty search record: {url}')
        indexed[url] = entry['meta']['title']
    if indexed != expected:
        raise ValueError(f'search index differs from public pages: {indexed}')
    for route in expected:
        target_file(site, site / 'index.html', route, base)
    metadata = json.loads((site / 'pagefind/pagefind-entry.json').read_text())
    if sum(language['page_count'] for language in metadata['languages'].values()) != len(expected):
        raise ValueError('Pagefind metadata count differs from public pages')
    if not (site / 'pagefind/pagefind.js').is_file():
        raise ValueError('missing Pagefind client')
    return len(indexed)


def inspect_site(site, base='/'):
    site = site.resolve()
    pages = {path: Page(path.read_text()) for path in site.rglob('*.html')}
    if not pages or site / 'index.html' not in pages:
        raise ValueError('built site has no index')
    resources = set()
    for path, page in pages.items():
        if page.headings != 1:
            raise ValueError(f'expected one page heading in {path.relative_to(site)}')
        for tag, attribute, reference in page.references:
            target = target_file(site, path, reference, base)
            if target is None:
                if attribute == 'src' or tag == 'link':
                    raise ValueError(f'external rendering asset: {reference}')
                continue
            fragment = unquote(urlsplit(reference).fragment)
            if fragment and target in pages and fragment not in pages[target].ids:
                raise ValueError(f'missing anchor {reference} from {path.relative_to(site)}')
            resources.add(target)
    for path in site.rglob('*.css'):
        for reference in re.findall(r'url\([\"\']?([^\)\"\']+)[\"\']?\)', path.read_text()):
            if reference.startswith('data:'):
                continue
            target = target_file(site, path, reference, base)
            if target is None:
                raise ValueError(f'external CSS asset: {reference}')
            resources.add(target)
    forbidden = {'PRD.md', 'PRD-0.1.md', 'research.md', 'implementation.md', 'portable-environments.md', 'plans'}
    for path in site.rglob('*'):
        if forbidden.intersection(path.relative_to(site).parts):
            raise ValueError(f'internal documentation included: {path}')
    for path in pages:
        text = path.read_text()
        if '/Users/davafons/' in text or '/home/davafons/' in text:
            raise ValueError(f'machine-specific artifact path included: {path}')
    for name, source in {
        'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
        'logo.svg': 'logo.svg',
        'demo-parcela.yaml': 'examples/docs-demo/parcela.yaml',
        'demo-frontend.html': 'examples/docs-demo/frontend/index.html',
        'demo-backend.json': 'examples/docs-demo/backend/status.json',
    }.items():
        if (site / 'assets' / name).read_bytes() != (ROOT / source).read_bytes():
            raise ValueError(f'download differs from canonical source: {name}')
    check_search(site, base)
    return pages, resources


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def check_http(site, pages, resources, base='/'):
    directory = site if base == '/' else site.parent
    handler = partial(QuietHandler, directory=str(directory))
    with ThreadingHTTPServer(('127.0.0.1', 0), handler) as server:
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        origin = f'http://127.0.0.1:{server.server_port}'
        prefix = origin + base
        try:
            for path, page in pages.items():
                page_url = prefix + path.relative_to(site).as_posix()
                with urlopen(page_url, timeout=3) as response:
                    if response.read() != path.read_bytes():
                        raise ValueError(f'served page differs: {path}')
                for _, _, reference in page.references:
                    if urlsplit(reference).scheme or urlsplit(reference).netloc:
                        continue
                    url = urljoin(page_url, reference)
                    if not urlsplit(url).path.startswith(base):
                        raise ValueError(f'link leaves project prefix: {reference}')
                    with urlopen(url, timeout=3) as response:
                        if response.status != 200:
                            raise ValueError(f'failed resource: {url}')
                        response.read()
            resources = resources | {path for path in (site / 'pagefind').rglob('*') if path.is_file()}
            for path in resources:
                with urlopen(prefix + path.relative_to(site).as_posix(), timeout=3) as response:
                    if response.read() != path.read_bytes():
                        raise ValueError(f'served resource differs: {path}')
        finally:
            server.shutdown()
            thread.join(timeout=3)
            if thread.is_alive():
                raise ValueError('documentation test server did not stop')
        port = server.server_port
    with socket.socket() as probe:
        if probe.connect_ex(('127.0.0.1', port)) == 0:
            raise ValueError('documentation test listener survives')
    return {'prefix': base, 'pages': len(pages), 'resources': len(resources), 'listener_closed': True}


def check_public_sources(directory):
    source = directory / 'source'
    shutil.copytree(ROOT / 'docs/site', source)
    program = "import {validatePublicSources} from './content.mjs'; validatePublicSources(process.argv[1]);"

    def refused(message):
        result = subprocess.run(['node', '--input-type=module', '-e', program, str(source)],
                                cwd=WEBSITE, capture_output=True, text=True, timeout=10)
        if result.returncode == 0 or message not in result.stderr:
            raise ValueError(f'public source refusal failed: {result.stdout} {result.stderr}')

    unexpected = source / 'unlisted-private.txt'
    unexpected.write_text('private fixture')
    refused('Undeclared public documentation source: unlisted-private.txt')
    unexpected.unlink()
    outside = directory / 'private.md'
    outside.write_text('# Private fixture')
    (source / 'index.md').unlink()
    (source / 'index.md').symlink_to(outside)
    refused('Public documentation source escapes its root: index.md')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='parcela-docs-') as directory:
        directory = Path(directory).resolve()
        check_public_sources(directory)
        results = []
        for base, name in [('/', 'root'), ('/parcela/', 'parcela'), ('/docs/', 'docs')]:
            site = directory / name
            subprocess.run(['npm', 'run', 'build'], cwd=WEBSITE, check=True, timeout=120,
                           env={**os.environ, 'DOCS_BASE': base, 'DOCS_OUTPUT': str(site)})
            pages, resources = inspect_site(site, base)
            results.append(check_http(site, pages, resources, base))
        report = {'complete': True, 'hosting': results, 'internal_docs_excluded': True,
                  'undeclared_and_external_sources_refused': True, 'canonical_downloads': True,
                  'search_index_pages': check_search(site, base), 'declared_theme_contrast': check_theme_colors(),
                  'browser_visual_check': False}
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
