"""The release workflow runs every CI native suite and publishes reviewed notes.

These checks read the workflow files as text; they never run a release, create a
tag or contact GitHub.
"""
from pathlib import Path
import re
import unittest

REPO = Path(__file__).resolve().parents[1]
CI = (REPO / '.github' / 'workflows' / 'ci.yml').read_text(encoding='utf-8')
RELEASE = (REPO / '.github' / 'workflows' / 'release.yml').read_text(encoding='utf-8')
VERSION = re.search(r'^version = "([^"]+)"', (REPO / 'Cargo.toml').read_text(encoding='utf-8'), re.M).group(1)


def ci_native_suites():
    return set(re.findall(r'--bin rust-browser (native_[a-z0-9_]+) -- --ignored', CI))


def release_native_suites():
    match = re.search(r"foreach \(\$test in @\(([^)]*)\)\)", RELEASE)
    if not match:
        raise AssertionError('release.yml has no native suite list')
    return set(re.findall(r"'(native_[a-z0-9_]+)'", match.group(1)))


class SuiteListTests(unittest.TestCase):
    def test_release_runs_every_native_suite_ci_runs(self):
        self.assertTrue(ci_native_suites(), 'no native suites found in ci.yml')
        self.assertEqual(release_native_suites(), ci_native_suites())

    def test_release_runs_the_two_process_suites_and_daily_runs(self):
        self.assertIn('--test single_instance -- --ignored', RELEASE)
        self.assertEqual(RELEASE.count('windows-daily.ps1'), 1)
        self.assertIn('foreach ($attempt in 1..3)', RELEASE)


class PublishTests(unittest.TestCase):
    def test_publishing_needs_the_manual_input_and_an_existing_tag(self):
        self.assertIn("if: github.event_name == 'workflow_dispatch' && inputs.publish", RELEASE)
        self.assertIn('default: false', RELEASE)
        self.assertIn('--verify-tag', RELEASE)
        self.assertNotIn('git tag', RELEASE)
        self.assertNotIn('git push', RELEASE)

    def test_release_notes_come_from_the_reviewed_file(self):
        self.assertNotIn('--generate-notes', RELEASE)
        self.assertIn('--notes-file', RELEASE)
        self.assertIn('release-notes/$env:RELEASE_TAG.md', RELEASE)

    def test_the_current_version_has_release_notes(self):
        notes = REPO / 'release-notes' / f'v{VERSION}.md'
        self.assertTrue(notes.is_file(), f'missing {notes.name}')
        text = notes.read_text(encoding='utf-8')
        self.assertNotIn('\u2014', text, 'no em dashes in release notes')
        for required in ['unsigned', 'SmartScreen', 'WebView2', 'Visual C++',
                         'Known limitations', 'SHA-256', 'Report a vulnerability']:
            self.assertIn(required, text, required)

    def test_artifact_names_match_the_package_scripts(self):
        package = (REPO / 'packaging' / 'windows' / 'package.ps1').read_text(encoding='utf-8')
        installer = (REPO / 'packaging' / 'windows' / 'build-installer.ps1').read_text(encoding='utf-8')
        self.assertIn('"rust-browser-v$Version-windows-x64.zip"', package)
        self.assertIn('"rust-browser-v$Version-windows-x64-setup.exe"', installer)
        self.assertIn('"rust-browser-v$Version-windows-x64-release.json"', installer)
        self.assertIn('$prefix = "rust-browser-$env:RELEASE_TAG-windows-x64"', RELEASE)
        for suffix in ['.zip', '.zip.sha256', '-setup.exe', '-setup.exe.sha256', '-release.json']:
            self.assertIn(f'"$prefix{suffix}"', RELEASE, suffix)


if __name__ == '__main__':
    unittest.main()
