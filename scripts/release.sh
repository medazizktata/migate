#!/usr/bin/env bash
# Prepare a release locally: all checks, the version bump, a CHANGELOG.md section from the commit subjects since the
# last release, a commit and an annotated tag. Nothing is pushed: `just publish` does that, so a release never leaves
# the machine by accident. Pushing the tag runs .github/workflows/release.yml.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
fail() { echo "release: $*" >&2; exit 1; }

version="${1:?usage: release.sh X.Y.Z}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "$version is not a version X.Y.Z"
[ "$(git branch --show-current)" = main ] || fail "release from main (on $(git branch --show-current))"
[ -z "$(git status --porcelain)" ] || fail "the working tree has uncommitted changes"
! git rev-parse -q --verify "refs/tags/v$version" >/dev/null || fail "v$version already exists"
current="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
[ "$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -1)" = "$version" ] && [ "$current" != "$version" ] ||
  fail "$version is not newer than the current $current"

bash scripts/check.sh

last="$(git describe --tags --abbrev=0 2>/dev/null || true)"
subjects="$(git log --no-merges --format='- %s' "${last:+$last..}HEAD")"
[ -n "$subjects" ] || fail "nothing to release since $last"

perl -0pi -e "s/^version = \"\Q$current\E\"/version = \"$version\"/m" Cargo.toml
cargo update --workspace --quiet
section="$(printf '## v%s (%s)\n\n%s\n' "$version" "$(date -u +%Y-%m-%d)" "$subjects")"
if [ -f CHANGELOG.md ]; then
  { head -n 2 CHANGELOG.md; echo "$section"; echo; tail -n +3 CHANGELOG.md; } > CHANGELOG.md.new
else
  printf '# Changelog\n\n%s\n' "$section" > CHANGELOG.md.new
fi
mv CHANGELOG.md.new CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -q -m "Release v$version"
git tag -a "v$version" -m "migate v$version"
echo "release: prepared v$version (commit and tag, not pushed). Review CHANGELOG.md, then: just publish"
