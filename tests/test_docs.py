import subprocess
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from scripts.check_docs import WEBSITE, Page, check_theme_colors, contrast_ratio, inspect_site, target_file, validate_search_records


class DocumentationSources(unittest.TestCase):
    def test_nested_snippets_and_vue_expressions_are_refused(self):
        program = "import {validateMarkdownSource} from './content.mjs'; validateMarkdownSource(process.argv[1], 'fixture.md');"
        for payload in ['> <<< /private/sentinel.txt', '- <<< /private/sentinel.txt',
                        '> - <<< /private/sentinel.txt', '{{ 7 * 7 }}',
                        "{{ ''.constructor.constructor('return process')() }}"]:
            with self.subTest(payload=payload):
                result = subprocess.run(['node', '--input-type=module', '-e', program, '--', payload],
                                        cwd=WEBSITE, capture_output=True, text=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Public Markdown cannot load extra sources', result.stderr)


class DocumentationLinks(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.site = Path(self.temporary.name).resolve()
        self.current = self.site / 'guide/index.html'
        self.current.parent.mkdir()
        self.current.write_text('<h1 id="guide">Guide</h1>')

    def test_relative_directory_encoded_resource_and_fragment_resolve(self):
        root = self.site / 'index.html'
        root.write_text('<h1 id="start">Start</h1>')
        asset = self.site / 'two words.css'
        asset.write_text('body {}')
        self.assertEqual(target_file(self.site, self.current, '../#start'), root)
        self.assertEqual(target_file(self.site, self.current, '../two%20words.css?v=1'), asset)
        self.assertEqual(target_file(self.site, self.current, '#guide'), self.current)

    def test_missing_absolute_and_escaping_targets_are_rejected(self):
        for reference in ['missing.html', '/assets/style.css', '../../outside.html', '../%2e%2e/outside.html']:
            with self.subTest(reference=reference), self.assertRaises(ValueError):
                target_file(self.site, self.current, reference, '/parcela/')

    def test_root_absolute_targets_are_scoped_to_the_build_base(self):
        root = self.site / 'index.html'
        root.write_text('<h1>Start</h1>')
        self.assertEqual(target_file(self.site, self.current, '/parcela/', '/parcela/'), root)
        self.assertEqual(target_file(self.site, self.current, '/', '/'), root)
        for reference in ['/assets/style.css', '/parcelax/', '/parcela/../index.html', '/parcela/%2e%2e/index.html']:
            with self.subTest(reference=reference), self.assertRaises(ValueError):
                target_file(self.site, self.current, reference, '/parcela/')

    def test_canonical_links_are_metadata_not_external_rendering_assets(self):
        page = Page('<h1>Start</h1><link rel="canonical" href="https://example.test/parcela/">')
        self.assertEqual(page.references, [])

    def test_rendered_code_preserves_shell_characters(self):
        page = Page("<pre><code>test &quot;$value&quot; &amp;&amp; printf '&lt;ok&gt;'</code></pre>")
        self.assertEqual(page.code_blocks, ["test \"$value\" && printf '<ok>'"])

    def test_unsupported_schemes_are_not_treated_as_external_web_links(self):
        for reference in ['javascript:alert(1)', 'file:///private/file', 'data:text/html,hello', '//example.test/']:
            with self.subTest(reference=reference), self.assertRaises(ValueError):
                target_file(self.site, self.current, reference)
        self.assertIsNone(target_file(self.site, self.current, 'https://example.test/'))

    def test_color_contrast_uses_linear_srgb_luminance(self):
        self.assertAlmostEqual(contrast_ratio('#ffffff', '#000000'), 21)
        self.assertAlmostEqual(contrast_ratio('#ffffff', '#ffffff'), 1)
        self.assertAlmostEqual(contrast_ratio('#777777', '#ffffff'), 4.478089, places=5)

    def test_color_contrast_is_independent_of_foreground_order(self):
        self.assertAlmostEqual(contrast_ratio('#000000', '#ffffff'), 21)
        self.assertAlmostEqual(contrast_ratio('#ffffff', '#777777'), 4.478089, places=5)

    def test_unreadable_declared_theme_is_rejected(self):
        (self.site / 'src').mkdir()
        (self.site / 'src/parcela.css').write_text(
            ':root.dark { --vp-c-text-1: #111318; --vp-c-bg: #111318; }'
            ':root { --vp-c-text-1: #ffffff; --vp-c-bg: #ffffff; }')
        with patch('scripts.check_docs.WEBSITE', self.site):
            with self.assertRaisesRegex(ValueError, 'dark body contrast 1.00 is below 4.5'):
                check_theme_colors()

    def test_duplicate_ids_fail_instead_of_hiding_anchor_ambiguity(self):
        with self.assertRaisesRegex(ValueError, 'duplicate HTML ID'):
            Page('<h1 id="same">One</h1><h2 id="same">Two</h2>')

    def test_actual_rendered_missing_anchor_is_rejected(self):
        (self.site / 'index.html').write_text('<h1>Start</h1><a href="guide/#absent">Guide</a>')
        with self.assertRaisesRegex(ValueError, 'missing anchor'):
            inspect_site(self.site)

    def test_external_rendering_assets_are_rejected(self):
        (self.site / 'index.html').write_text('<h1>Start</h1><script src="https://example.test/render.js"></script>')
        with self.assertRaisesRegex(ValueError, 'external rendering asset'):
            inspect_site(self.site)

    def test_internal_plan_directory_is_not_a_public_asset(self):
        (self.site / 'index.html').write_text('<h1>Start</h1>')
        (self.site / 'plans').mkdir()
        (self.site / 'plans/private.txt').write_text('private')
        with self.assertRaisesRegex(ValueError, 'internal documentation included'):
            inspect_site(self.site)


class DocumentationSearch(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.site = Path(self.temporary.name)
        self.expected = {}
        self.documents = []
        for slug, title in [('agents', 'Work with an agent'), ('transfer', 'Paused checkpoints'),
                            ('first-environment', 'First environment')]:
            directory = self.site / slug
            directory.mkdir()
            (directory / 'index.html').write_text(f'<h1 id="start">{title}</h1>')
            route = f'/docs/{slug}/'
            self.expected[route] = title
            self.documents.append({'id': route + '#start', 'title': title, 'titles': []})
        self.data = {'documentCount': 3, 'documents': self.documents, 'queries': {
            'Claude': ['/docs/agents/#start'],
            'checkpoint': ['/docs/transfer/#start'],
            'frontend': ['/docs/first-environment/#start'],
        }}

    def check(self):
        return validate_search_records(self.site, '/docs/', self.expected, self.data)

    def test_every_page_and_real_query_result_is_required(self):
        self.assertEqual(self.check(), 3)

    def test_missing_public_page_is_rejected(self):
        self.documents.pop()
        self.data['documentCount'] = 2
        with self.assertRaisesRegex(ValueError, 'search index differs'):
            self.check()

    def test_empty_index_is_rejected(self):
        self.documents.clear()
        self.data['documentCount'] = 0
        with self.assertRaisesRegex(ValueError, 'empty or inconsistent'):
            self.check()

    def test_count_mismatch_is_rejected(self):
        self.data['documentCount'] = 4
        with self.assertRaisesRegex(ValueError, 'empty or inconsistent'):
            self.check()

    def test_duplicate_record_is_rejected(self):
        self.documents.append(self.documents[0].copy())
        self.data['documentCount'] = 4
        with self.assertRaisesRegex(ValueError, 'duplicate or unexpected'):
            self.check()

    def test_private_or_unlisted_page_is_rejected(self):
        self.documents[0]['id'] = '/docs/plans/private/#start'
        with self.assertRaisesRegex(ValueError, 'duplicate or unexpected'):
            self.check()

    def test_search_anchor_must_exist_in_actual_html(self):
        self.documents[0]['id'] = '/docs/agents/#absent'
        with self.assertRaisesRegex(ValueError, 'missing search anchor'):
            self.check()

    def test_title_must_match_canonical_page(self):
        self.documents[0]['title'] = 'Different page'
        with self.assertRaisesRegex(ValueError, 'search title differs'):
            self.check()

    def test_empty_query_results_are_not_search_proof(self):
        self.data['queries']['Claude'] = []
        with self.assertRaisesRegex(ValueError, 'no expected result: Claude'):
            self.check()


if __name__ == '__main__':
    unittest.main()
