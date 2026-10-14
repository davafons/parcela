import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

from scripts.check_docs import Page, check_theme_colors, contrast_ratio, inspect_site, target_file


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
            ':root { --sl-color-text: #111318; --sl-color-bg: #111318; }'
            ':root[data-theme="light"] { --sl-color-text: #ffffff; --sl-color-bg: #ffffff; }')
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


if __name__ == '__main__':
    unittest.main()
