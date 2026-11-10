"""The CI change classifier skips Windows jobs only for unpackaged documentation."""
import importlib.util
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('ci_scope', REPO / '.github' / 'scripts' / 'ci_scope.py')
SCOPE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCOPE)
A = 'a' * 40
B = 'b' * 40


class DocsOnlyTests(unittest.TestCase):
    def test_unpackaged_markdown_anywhere_is_docs_only(self):
        self.assertTrue(SCOPE.is_docs_only(['CI.md', 'validation/RELEASE_CRITERIA.md',
                                            'DEVELOPMENT_CHECKPOINT.md', 'docs/x/Notes.MD']))

    def test_packaged_documentation_runs_everything(self):
        for name in sorted(SCOPE.PACKAGED_DOCS):
            self.assertFalse(SCOPE.is_docs_only([name]), name)
            self.assertFalse(SCOPE.is_docs_only(['CI.md', name.lower()]), name)

    def test_anything_that_is_not_markdown_runs_everything(self):
        for path in ['src/app.rs', 'Cargo.lock', 'Cargo.toml', 'build.rs', 'LICENSE',
                     '.github/workflows/ci.yml', '.github/scripts/ci_scope.py',
                     '.github/dependabot.yml', 'tests/windows_package.ps1',
                     'packaging/windows/installer.iss', 'packaging/windows/installer-notice.txt',
                     'validation/site-survey-sites.txt', 'src/ui/index.html', 'notes.md.rs']:
            self.assertFalse(SCOPE.is_docs_only(['CI.md', path]), path)

    def test_empty_or_malformed_change_sets_run_everything(self):
        self.assertFalse(SCOPE.is_docs_only([]))
        self.assertFalse(SCOPE.is_docs_only(['']))
        self.assertFalse(SCOPE.is_docs_only(['docs\\x.md']))

    def test_packaged_list_matches_the_package_and_installer(self):
        package = (REPO / 'packaging' / 'windows' / 'package.ps1').read_text(encoding='utf-8')
        sources = re.search(r"\$sources=@\(\$exe\)\+@\(([^)]*)\)", package)
        self.assertIsNotNone(sources)
        listed = set(re.findall(r"'([^']+)'", sources.group(1)))
        self.assertEqual({n for n in listed if n.lower().endswith('.md')}, set(SCOPE.PACKAGED_DOCS))
        installer = (REPO / 'packaging' / 'windows' / 'installer.iss').read_text(encoding='utf-8')
        shipped = set(re.findall(r'Source: "\{#PayloadRoot\}\\([^"]+\.md)"', installer))
        self.assertEqual(shipped, set(SCOPE.PACKAGED_DOCS))
        check = (REPO / 'tests' / 'windows_package.ps1').read_text(encoding='utf-8')
        expected = re.search(r"\$expected=@\(([^)]*)\)", check)
        self.assertEqual({n for n in re.findall(r"'([^']+)'", expected.group(1))
                          if n.endswith('.md')}, set(SCOPE.PACKAGED_DOCS))


class DecisionTests(unittest.TestCase):
    def test_only_pull_requests_and_pushes_are_diffed(self):
        docs = lambda base, head: ['CI.md']
        self.assertFalse(SCOPE.full_run_needed('pull_request', A, B, docs))
        self.assertFalse(SCOPE.full_run_needed('push', A, B, docs))
        for event in ['workflow_dispatch', 'schedule', 'merge_group', '']:
            self.assertTrue(SCOPE.full_run_needed(event, A, B, docs), event)

    def test_unusable_commits_run_everything(self):
        docs = lambda base, head: ['CI.md']
        for base in ['', '0' * 40, 'abc', 'g' * 40, A + 'a']:
            self.assertTrue(SCOPE.full_run_needed('push', base, B, docs), base)
            self.assertTrue(SCOPE.full_run_needed('push', A, base, docs), base)

    def test_a_failed_diff_runs_everything(self):
        def broken(base, head):
            raise subprocess.CalledProcessError(128, 'git')
        self.assertTrue(SCOPE.full_run_needed('pull_request', A, B, broken))

    def test_verdict_requires_every_needed_job(self):
        self.assertTrue(SCOPE.verdict('success', 'true', 'success', 'success'))
        self.assertTrue(SCOPE.verdict('success', 'false', 'skipped', 'skipped'))
        for case in [('success', 'true', 'skipped', 'skipped'),
                     ('success', 'true', 'success', 'failure'),
                     ('success', 'true', 'cancelled', 'success'),
                     ('success', 'false', 'success', 'skipped'),
                     ('success', 'false', 'failure', 'skipped'),
                     ('failure', 'false', 'skipped', 'skipped'),
                     ('skipped', 'true', 'success', 'success'),
                     ('success', '', 'success', 'success'),
                     ('success', 'yes', 'skipped', 'skipped')]:
            self.assertFalse(SCOPE.verdict(*case), case)


@unittest.skipUnless(shutil.which('git'), 'git is required')
class GitDiffTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.email', 'ci@example.invalid')
        self.git('config', 'user.name', 'ci')
        self.write('src/x.rs', 'fn main() {}\n')
        self.write('CI.md', 'notes\n')
        self.base = self.commit('base')

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.root), *args], check=True,
                              capture_output=True).stdout.decode().strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')

    def commit(self, message):
        self.git('add', '-A')
        self.git('commit', '-q', '-m', message)
        return self.git('rev-parse', 'HEAD')

    def decide(self, head):
        run = lambda args, **kw: subprocess.run(['git', '-C', str(self.root), *args[1:]], **kw)
        return SCOPE.full_run_needed('pull_request', self.base, head,
                                     lambda b, h: SCOPE.changed_paths(b, h, run))

    def test_docs_commit_is_skipped_and_code_commit_is_not(self):
        self.write('CI.md', 'more notes\n')
        self.assertFalse(self.decide(self.commit('docs')))
        self.write('src/x.rs', 'fn main() { }\n')
        self.assertTrue(self.decide(self.commit('code')))

    def test_whole_pull_request_is_classified_not_only_the_last_push(self):
        self.write('src/x.rs', 'fn main() { }\n')
        self.commit('code')
        self.write('CI.md', 'more notes\n')
        self.assertTrue(self.decide(self.commit('docs on top of code')))

    def test_renaming_code_to_markdown_is_not_docs_only(self):
        self.git('mv', 'src/x.rs', 'src/x.md')
        self.assertTrue(self.decide(self.commit('rename')))

    def test_unknown_commit_runs_everything(self):
        self.assertTrue(self.decide('c' * 40))


if __name__ == '__main__':
    unittest.main()
