"""Decide whether a CI run needs the Windows jobs, and judge the aggregate result.

The Windows jobs are skipped only when every file the change touches is Markdown
documentation that no build, test or package reads. Anything else, and any doubt
(an unknown event, a missing or all-zero base commit, an empty or failed diff), runs
everything.

    ci_scope.py scope    # reads EVENT, BASE, HEAD; writes full=true|false to GITHUB_OUTPUT
    ci_scope.py result SCOPE_RESULT FULL WINDOWS_RESULT RELEASE_RESULT

Standard library only. Unit tests: validation/test_ci_scope.py.
"""

import os
import subprocess
import sys

# Copied into the portable package and the installer; tests/windows_package.ps1
# checks their links. validation/test_ci_scope.py keeps this list equal to
# packaging/windows/package.ps1.
PACKAGED_DOCS = frozenset(
    {"README.md", "CHANGELOG.md", "SECURITY.md", "VALIDATION.md", "INSTALL_WINDOWS.md"}
)
_PACKAGED_LOWER = frozenset(name.lower() for name in PACKAGED_DOCS)
# Events whose change set is a pull request or a push to main. Anything else, such
# as workflow_dispatch, always runs everything.
DIFFED_EVENTS = frozenset({"pull_request", "push"})


def is_docs_only(paths):
    """True only for a non-empty change made entirely of unpackaged Markdown."""
    if not paths:
        return False
    for path in paths:
        if not path or "\\" in path or not path.lower().endswith(".md"):
            return False
        # Windows file names ignore case, so "readme.md" is still the packaged file.
        if path.lower() in _PACKAGED_LOWER:
            return False
    return True


def _usable_commit(sha):
    return bool(sha) and len(sha) == 40 and set(sha) != {"0"} and all(
        c in "0123456789abcdef" for c in sha.lower()
    )


def changed_paths(base, head, run=subprocess.run):
    """Files that differ between the merge base of `base` and `head`, and `head`.

    Renames are listed as a deletion and an addition, so moving code into a
    Markdown file still names the code path.
    """
    result = run(
        ["git", "diff", "--no-renames", "--name-only", "-z", f"{base}...{head}"],
        capture_output=True,
        check=True,
    )
    return [p for p in result.stdout.decode("utf-8").split("\0") if p]


def full_run_needed(event, base, head, diff=changed_paths):
    if event not in DIFFED_EVENTS or not (_usable_commit(base) and _usable_commit(head)):
        return True
    try:
        return not is_docs_only(diff(base, head))
    except Exception:  # noqa: BLE001 - any failure to classify runs everything
        return True


def verdict(scope_result, full, windows_result, release_result):
    """The aggregate "CI result": success only if every job that had to run passed."""
    if scope_result != "success" or full not in ("true", "false"):
        return False
    expected = "success" if full == "true" else "skipped"
    return windows_result == expected and release_result == expected


def main(argv):
    if len(argv) == 2 and argv[1] == "scope":
        try:
            full = full_run_needed(
                os.environ.get("EVENT", ""),
                os.environ.get("BASE", ""),
                os.environ.get("HEAD", ""),
            )
        except Exception:  # noqa: BLE001
            full = True
        line = f"full={'true' if full else 'false'}"
        print(line)
        output = os.environ.get("GITHUB_OUTPUT")
        if output:
            with open(output, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        return 0
    if len(argv) == 6 and argv[1] == "result":
        ok = verdict(*argv[2:])
        print(f"scope={argv[2]} full={argv[3]} windows={argv[4]} release={argv[5]}: "
              f"{'success' if ok else 'failure'}")
        return 0 if ok else 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
