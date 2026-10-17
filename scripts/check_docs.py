"""Check VitePress artifacts, public sources and root/project-subpath HTTP serving."""
import argparse
from functools import partial
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
        self.code_blocks = []
        self.code = None
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
        if tag == 'pre':
            self.code = []
        for attribute in ['href', 'src']:
            if attribute in attributes and attributes.get('rel') != 'canonical':
                self.references.append((tag, attribute, attributes[attribute]))

    def handle_data(self, text):
        if self.code is not None:
            self.code.append(text)

    def handle_endtag(self, tag):
        if tag == 'pre' and self.code is not None:
            self.code_blocks.append(''.join(self.code))
            self.code = None


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
    palettes = {'dark' if mode else 'light': dict(re.findall(r'(--vp-[\w-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;', body))
                for mode, body in re.findall(r':root(\.dark)?\s*\{([^}]+)\}', text)}
    if set(palettes) != {'dark', 'light'}:
        raise ValueError('expected explicit dark and light palettes')
    pairs = [
        ('body', 'c-text-1', 'c-bg', 4.5),
        ('heading', 'c-text-1', 'c-bg', 4.5),
        ('sidebar label', 'c-text-2', 'c-bg-alt', 4.5),
        ('link', 'c-brand-1', 'c-bg', 4.5),
        ('selected navigation', 'c-brand-1', 'c-bg-alt', 4.5),
        ('inline code', 'code-color', 'code-bg', 4.5),
        ('accent control', 'button-brand-text', 'c-brand-3', 4.5),
        ('focus on page', 'c-brand-1', 'c-bg', 3),
        ('focus in header', 'c-brand-1', 'c-bg', 3),
    ]
    results = {}
    for mode, colors in palettes.items():
        results[mode] = {}
        for label, foreground, background, minimum in pairs:
            ratio = contrast_ratio(colors[f'--vp-{foreground}'], colors[f'--vp-{background}'])
            if ratio < minimum:
                raise ValueError(f'{mode} {label} contrast {ratio:.2f} is below {minimum}')
            results[mode][label] = round(ratio, 2)
    return results


def validate_search_records(site, base, expected, data):
    documents = data['documents']
    if not documents or data['documentCount'] != len(documents):
        raise ValueError('empty or inconsistent local search index')
    identifiers = set()
    indexed = set()
    for entry in documents:
        identifier = entry['id']
        route, _, fragment = identifier.partition('#')
        if identifier in identifiers or route not in expected:
            raise ValueError(f'duplicate or unexpected search record: {identifier}')
        identifiers.add(identifier)
        target = target_file(site, site / 'index.html', route, base)
        if fragment and unquote(fragment) not in Page(target.read_text()).ids:
            raise ValueError(f'missing search anchor: {identifier}')
        titles = entry['titles'] or [entry['title']]
        if titles[0] != expected[route]:
            raise ValueError(f'search title differs from public page: {identifier}')
        indexed.add(route)
    if indexed != set(expected):
        raise ValueError('search index differs from public pages')
    for term, slug in [('Claude', 'worktrees'), ('worktree', 'worktrees'), ('ticker', 'getting-started'), ('prune', 'cli')]:
        if not any(identifier.partition('#')[0] == f'{base}{slug}/' for identifier in data['queries'].get(term, [])):
            raise ValueError(f'local search query has no expected result: {term}')
    return len(indexed)


def check_search(site, base):
    expected = json.loads((site / 'documentation-pages.json').read_text())
    indexes = list((site / 'assets').rglob('@localSearchIndex*.js'))
    if len(indexes) != 1:
        raise ValueError('expected one bundled local search index')
    if not list((site / 'assets').rglob('VPLocalSearchBox*.js')):
        raise ValueError('missing local search client')
    result = subprocess.run(['node', str(WEBSITE / 'check-search.mjs'), str(indexes[0])],
                            check=True, capture_output=True, text=True, timeout=15)
    return validate_search_records(site, base, expected, json.loads(result.stdout))


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
    for path in pages:
        text = path.read_text()
        if '/Users/davafons/' in text or '/home/davafons/' in text:
            raise ValueError(f'machine-specific artifact path included: {path}')
    for name, source in {
        'parcela-skill.txt': '.agents/skills/parcela/SKILL.md',
        'logo.svg': 'assets/brand/logo.svg',
        'third-party-notices.txt': 'docs/website/THIRD_PARTY_NOTICES.txt',
    }.items():
        if (site / 'assets' / name).read_bytes() != (ROOT / source).read_bytes():
            raise ValueError(f'download differs from canonical source: {name}')
    section = (ROOT / 'docs/site/worktrees.md').read_text().split('### Install the skill for Claude Code\n', 1)[1]
    script = re.search(r'```sh\n(.*?)\n```', section, re.S).group(1)
    if script not in [block.rstrip('\n') for block in pages[site / 'worktrees/index.html'].code_blocks]:
        raise ValueError('rendered Claude installation block differs from canonical Markdown')
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
            resources = resources | {path for path in site.rglob('*') if path.is_file()}
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
    program = """
import {validateMarkdownSource} from './content.mjs';
validateMarkdownSource(process.argv[1], 'fixture.md');
"""
    for markdown in ['<!--@include: ../../private.md-->', '<<< ../../private.txt',
                     '<<<../../private.txt', '<script setup>import secret from \"../../private.json\"</script>']:
        result = subprocess.run(['node', '--input-type=module', '-e', program, markdown],
                                cwd=WEBSITE, capture_output=True, text=True, timeout=10)
        if result.returncode == 0 or 'Public Markdown cannot load extra sources' not in result.stderr:
            raise ValueError('public Markdown source-loading refusal failed')


def check_source_build_refusals(directory):
    repository = directory / 'boundary-fixture'
    website = repository / 'docs/website'
    shutil.copytree(WEBSITE, website, ignore=shutil.ignore_patterns('node_modules', '.generated'))
    (website / 'node_modules').symlink_to(WEBSITE / 'node_modules', target_is_directory=True)
    program = "import {groups, downloads} from './content.mjs'; console.log(JSON.stringify([...groups.flatMap(g => g.pages.map(p => p[2])), ...Object.values(downloads)]));"
    files = json.loads(subprocess.check_output(['node', '--input-type=module', '-e', program], cwd=WEBSITE, text=True))
    for name in files:
        destination = repository / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, destination)
    sentinel = directory / 'private-sentinel.txt'
    sentinel.write_text('PARCELA_UNDECLARED_BUILD_SENTINEL_759143')
    page = repository / 'docs/site/index.md'
    original = page.read_text()
    expression = "''.constructor.constructor('return process')().getBuiltinModule('fs').readFileSync(" + json.dumps(str(sentinel)) + ", 'utf8')"
    payloads = {
        'blockquote-snippet': f'> <<< {sentinel}',
        'list-snippet': f'- <<< {sentinel}',
        'nested-snippet': f'> - <<< {sentinel}',
        'vue-expression': '{{ 7 * 7 }}',
        'vue-file-read': '{{ ' + expression + ' }}',
    }
    for name, payload in payloads.items():
        page.write_text(original + '\n\n' + payload + '\n')
        result = subprocess.run(['npm', 'run', 'build'], cwd=website, capture_output=True, text=True, timeout=120,
                                env={**os.environ, 'DOCS_BASE': '/docs/', 'DOCS_OUTPUT': str(directory / name)})
        (directory / f'{name}.log').write_text(result.stdout + result.stderr)
        if result.returncode == 0 or 'Public Markdown cannot load extra sources' not in result.stderr:
            raise ValueError(f'actual build did not refuse {name}; see {directory / (name + ".log")}')
    page.write_text(original + '\n\n\\{\\{ ' + expression + ' \\}\\}\n')
    output = directory / 'escaped-vue-expression'
    result = subprocess.run(['npm', 'run', 'build'], cwd=website, capture_output=True, text=True, timeout=120,
                            env={**os.environ, 'DOCS_BASE': '/docs/', 'DOCS_OUTPUT': str(output)})
    (directory / 'escaped-vue-expression.log').write_text(result.stdout + result.stderr)
    if result.returncode != 0:
        raise ValueError('escaped Vue expression did not build as literal Markdown')
    if 'constructor' not in (output / 'index.html').read_text():
        raise ValueError('escaped Vue expression was omitted instead of rendered as literal Markdown')
    for path in output.rglob('*'):
        if path.is_file() and sentinel.read_bytes() in path.read_bytes():
            raise ValueError(f'escaped Vue expression executed during rendering: {path}')
    return {'refused_builds': len(payloads), 'inert_expression_builds': 1}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not (WEBSITE / 'node_modules').is_dir():
        raise SystemExit('docs/website/node_modules is missing; run npm ci --prefix docs/website first')
    with tempfile.TemporaryDirectory(prefix='parcela-docs-') as directory:
        directory = Path(directory).resolve()
        check_public_sources(directory)
        source_builds = check_source_build_refusals(directory)
        results = []
        for base, name in [('/', 'root'), ('/parcela/', 'parcela'), ('/docs/', 'docs')]:
            site = directory / name
            subprocess.run(['npm', 'run', 'build'], cwd=WEBSITE, check=True, timeout=120,
                           env={**os.environ, 'DOCS_BASE': base, 'DOCS_OUTPUT': str(site)})
            pages, resources = inspect_site(site, base)
            results.append(check_http(site, pages, resources, base))
        report = {'complete': True, 'hosting': results,
                  'undeclared_and_external_sources_refused': True, 'source_boundary_builds': source_builds,
                  'canonical_downloads': True,
                  'search_index_pages': check_search(site, base), 'declared_theme_contrast': check_theme_colors(),
                  'browser_visual_check': False}
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
