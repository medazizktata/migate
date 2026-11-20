#!/usr/bin/env bash
# Refuse to commit or push anything that should not leave this machine, even to a private repository:
#   - an author or committer email other than a GitHub noreply address;
#   - a secret (gitleaks's rules);
#   - a real-looking email address in added lines (example.com, *.invalid, *.test and noreply addresses are fine);
#   - an environment, key, credential or database file;
#   - anything matching the private patterns in .git/info/sensitive-patterns, one extended regex per line,
#     case-insensitive (names that must not be published, kept out of the repository because listing them here
#     would publish them).
#
# Usage: check-sensitive.sh staged                 the commit about to be made (pre-commit)
#        check-sensitive.sh range <rev-list args>  commits about to be pushed (pre-push)
#        check-sensitive.sh all                    the whole history
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

mode="${1:?usage: check-sensitive.sh staged | range <rev-list args> | all}"
shift
noreply='@users\.noreply\.github\.com$|^noreply@github\.com$'
ok_domains='@(example\.(com|org|net)|[a-z0-9.-]*\.(invalid|test|example|localhost)|users\.noreply\.github\.com)$'
email='[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}'
bad_files='(^|/)(\.env(\..*)?|\.dev\.vars|id_(rsa|ed25519|ecdsa)[^/]*|credentials[^/]*\.json)$|\.(pem|key|p12|pfx|db|sqlite3?)$'
private="$(git rev-parse --git-path info/sensitive-patterns)"
problems=0
fail() { echo "check-sensitive: $*" >&2; problems=$((problems + 1)); }

command -v gitleaks >/dev/null || { echo "check-sensitive: gitleaks is not installed (brew install gitleaks)" >&2; exit 1; }

# The commits in scope, and the text they add: diffs' added lines plus messages.
case "$mode" in
  staged)
    for who in GIT_AUTHOR_IDENT GIT_COMMITTER_IDENT; do
      addr="$(git var "$who" | sed -E 's/.*<([^>]*)>.*/\1/')"
      echo "$addr" | grep -q -E "$noreply" || fail "$who is $addr, not a GitHub noreply address (git config user.email)"
    done
    added="$(git diff --cached --no-color -U0 | grep -E '^\+' | grep -v -E '^\+\+\+ ' || true)"
    files="$(git diff --cached --name-only --diff-filter=AR)"
    gitleaks git --pre-commit --staged --no-banner --redact . >&2 || fail "gitleaks found a secret in the staged changes"
    ;;
  range | all)
    revs=("$@")
    [ "$mode" = all ] && revs=(--all)
    commits="$(git rev-list "${revs[@]}")"
    [ -n "$commits" ] || exit 0
    while read -r c; do
      for addr in "$(git show -s --format=%ae "$c")" "$(git show -s --format=%ce "$c")"; do
        echo "$addr" | grep -q -E "$noreply" || fail "commit $(git rev-parse --short "$c") carries $addr, not a GitHub noreply address"
      done
    done <<< "$commits"
    added="$(git log --no-color -p -U0 --format='%B' "${revs[@]}" | grep -v -E '^(\+\+\+|---) ' | grep -v -E '^-' || true)"
    files="$(git log --format= --name-only --diff-filter=AR "${revs[@]}" | sort -u)"
    gitleaks git --log-opts="${revs[*]}" --no-banner --redact . >&2 || fail "gitleaks found a secret in the commits"
    ;;
  *) echo "usage: check-sensitive.sh staged | range <rev-list args> | all" >&2; exit 2 ;;
esac

while read -r f; do
  [ -n "$f" ] && echo "$f" | grep -q -i -E "$bad_files" && fail "adds $f (environment, key, credential or database file)"
done <<< "$files"

found="$(echo "$added" | grep -o -E "$email" | grep -v -i -E "$ok_domains" | sort -u || true)"
[ -z "$found" ] || fail "adds real-looking email addresses: $(echo "$found" | tr '\n' ' ')"

if [ -f "$private" ]; then
  while read -r pattern; do
    [ -z "$pattern" ] || [ "${pattern:0:1}" = "#" ] && continue
    n="$(echo "$added" | grep -c -i -E -- "$pattern" || true)"
    [ "$n" = 0 ] || {
      fail "adds $n line(s) matching a private pattern (listed in $private), first:"
      echo "$added" | grep -i -E -- "$pattern" | head -2 | cut -c1-160 | sed 's/^/    /' >&2
    }
  done < "$private"
fi

[ "$problems" = 0 ] || { echo "check-sensitive: $problems problem(s); nothing was committed or pushed" >&2; exit 1; }
