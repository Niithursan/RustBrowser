"""The trusted browser document's scripts parse: one syntax error stops every view
(the toolbar, the tab column and the library column all load this document)."""
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
HTML = (REPO / 'src' / 'ui' / 'toolbar.html').read_text(encoding='utf-8')
SCRIPTS = re.findall(r'<script>(.*?)</script>', HTML, re.S)


class ScriptTests(unittest.TestCase):
    def test_the_document_has_its_inline_scripts(self):
        self.assertEqual(HTML.count('<script>'), len(SCRIPTS))
        self.assertGreaterEqual(len(SCRIPTS), 2, 'the main script and the address edit menu')
        self.assertNotIn('<script src', HTML)

    @unittest.skipUnless(shutil.which('node'), 'Node.js is needed to parse the scripts')
    def test_every_script_parses(self):
        with tempfile.TemporaryDirectory() as folder:
            for index, script in enumerate(SCRIPTS):
                path = Path(folder) / f'script{index}.js'
                path.write_text(script, encoding='utf-8')
                result = subprocess.run(['node', '--check', str(path)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, f'script {index}: {result.stderr}')


if __name__ == '__main__':
    unittest.main()
