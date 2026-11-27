#!/bin/sh
# The release workflow's Linux jobs build only an approved release: the same
# checks as its Windows job. The tag is vX.Y.Z, matches the Cargo version, is
# the checked-out commit and belongs to main's history.
# Usage: check-release-tag.sh TAG (run in a full clone with origin/main)
set -eu

tag=${1:?usage: check-release-tag.sh TAG}
fail() {
  echo "check-release-tag: $1" >&2
  exit 1
}

echo "$tag" | grep -Eqx 'v[0-9]+\.[0-9]+\.[0-9]+' || fail 'use an approved stable version tag'
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
[ "$tag" = "v$version" ] || fail "tag $tag does not match the Cargo version $version"
tag_commit=$(git rev-parse "refs/tags/$tag^{commit}") || fail "no tag $tag"
[ "$tag_commit" = "$(git rev-parse HEAD)" ] || fail 'the checkout is not the tag commit'
git merge-base --is-ancestor HEAD origin/main || fail 'the release is not on main'
