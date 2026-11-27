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
    matches = re.findall(r"foreach \(\$test in @\(([^)]*)\)\)", RELEASE)
    if not matches:
        raise AssertionError('release.yml has no native suite list')
    return {
        test
        for match in matches
        for test in re.findall(r"'(native_[a-z0-9_]+)'", match)
    }


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

    def test_release_metadata_states_the_current_library_schema(self):
        # v0.21.0 shipped metadata saying schema 3 while the build used schema 5:
        # the producer and the verifier must both follow library::SCHEMA_VERSION.
        library = (REPO / 'src' / 'library.rs').read_text(encoding='utf-8')
        schema = re.search(r'pub const SCHEMA_VERSION: i64 = (\d+);', library).group(1)
        installer = (REPO / 'packaging' / 'windows' / 'build-installer.ps1').read_text(encoding='utf-8')
        verifier = (REPO / 'packaging' / 'windows' / 'verify-release.ps1').read_text(encoding='utf-8')
        self.assertIn(f'profile_library={schema};', installer)
        self.assertIn(f'$record.profile_library -cne {schema}', verifier)

    def test_the_current_version_has_release_notes(self):
        notes = REPO / 'release-notes' / f'v{VERSION}.md'
        self.assertTrue(notes.is_file(), f'missing {notes.name}')
        text = notes.read_text(encoding='utf-8')
        self.assertNotIn('\u2014', text, 'no em dashes in release notes')
        for required in ['unsigned', 'SmartScreen', 'WebView2', 'Visual C++',
                         'Known limitations', 'SHA-256', 'Report a vulnerability',
                         # Linux packages ship from this release on.
                         'WebKitGTK', 'Ubuntu', 'Arch Linux', 'sha256sum']:
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


class LinuxPackageTests(unittest.TestCase):
    def job(self, name):
        match = re.search(rf'^  {name}:\n(.*?)(?=^  [a-z]+:\n|\Z)', RELEASE, re.M | re.S)
        self.assertIsNotNone(match, f'release.yml has no {name} job')
        return match.group(1)

    def test_linux_jobs_build_only_an_approved_tag(self):
        for name in ['linux', 'arch']:
            job = self.job(name)
            self.assertIn('sh packaging/linux/check-release-tag.sh "$RELEASE_TAG"', job, name)
            self.assertIn('fetch-depth: 0', job, name)
            self.assertIn('ref: ${{ inputs.release_tag || github.ref }}', job, name)
            # Clean release builds, as for Windows.
            self.assertNotIn('rust-cache', job, name)

    def test_linux_packages_are_installed_and_checked_before_upload(self):
        linux = self.job('linux')
        self.assertIn('cargo test --locked', linux)
        self.assertIn('docker run --rm -v "$PWD:/src:ro" ubuntu:24.04', linux)
        self.assertIn('check-installed.sh', linux)
        arch = self.job('arch')
        self.assertIn('container: archlinux:base-devel', arch)
        self.assertIn('build-from-checkout.sh', arch)
        self.assertIn('pacman -U --noconfirm', arch)
        self.assertIn('check-installed.sh', arch)

    def test_publishing_waits_for_and_attaches_every_package(self):
        publish = self.job('publish')
        self.assertIn('needs: [package, linux, arch]', publish)
        for artifact in ['windows-release-package', 'linux-release-package', 'arch-release-package']:
            self.assertIn(f"'{artifact}'", publish, artifact)
            self.assertIn(f'name: {artifact}', RELEASE, artifact)
        self.assertIn('$deb = "rust-browser-$env:RELEASE_TAG-linux-amd64.deb"', publish)
        self.assertIn('$arch = "rust-browser-$env:RELEASE_TAG-arch-x86_64.pkg.tar.zst"', publish)
        self.assertIn('name="rust-browser-$RELEASE_TAG-linux-amd64.deb"', self.job('linux'))
        self.assertIn('name="rust-browser-$RELEASE_TAG-arch-x86_64.pkg.tar.zst"', self.job('arch'))
        for name in ['$deb', '"$deb.sha256"', '$arch', '"$arch.sha256"']:
            self.assertIn(name, publish, name)
        # Every package's checksum is checked before anything is attached.
        self.assertIn('foreach ($name in @("$prefix.zip", "$prefix-setup.exe", $deb, $arch))', publish)


if __name__ == '__main__':
    unittest.main()
