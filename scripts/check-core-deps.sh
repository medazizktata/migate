#!/usr/bin/env bash
# The core dependency rule: migate-core's normal dependency tree may contain only the crates allowed below.
# A database driver, an async runtime or a git library in the core fails this check; so does any crate not yet
# approved. To add one, add it here in the same change, on purpose.
set -euo pipefail

allowed=(
  # SHA-256 for the ledger (sha2 0.10 and its tree; libc comes via cpufeatures on some targets)
  sha2 digest block-buffer crypto-common generic-array typenum cpufeatures cfg-if libc
  # Serialization of the schema model, so snapshots cross the JSON boundary to plug-ins (TASK-13). serde_derive and its
  # tree are compile-time code generation, not runtime I/O.
  serde serde_core serde_derive proc-macro2 quote syn unicode-ident
)

cargo=${CARGO:-cargo}
tree=$("$cargo" tree -p migate-core -e normal --prefix none --format '{p}')
deps=$(printf '%s\n' "$tree" | awk '{print $1}' | sort -u | grep -v -x 'migate-core' || true)

bad=()
while IFS= read -r dep; do
  [ -z "$dep" ] && continue
  ok=0
  for a in "${allowed[@]+"${allowed[@]}"}"; do [ "$dep" = "$a" ] && ok=1; done
  [ $ok -eq 1 ] || bad+=("$dep")
done <<< "$deps"

if [ ${#bad[@]} -gt 0 ]; then
  echo "error: migate-core depends on crates outside its allowlist: ${bad[*]}" >&2
  echo "The core must not depend on database drivers, async runtimes or git libraries." >&2
  echo "If a crate belongs in the core, add it to scripts/check-core-deps.sh on purpose." >&2
  exit 1
fi
echo "ok: migate-core depends only on allowed crates ($(printf '%s\n' "$deps" | grep -c . || true) found)"
