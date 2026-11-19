#!/bin/bash
# One pglite-socket cycle: the server starts PGlite in memory, runs migrate_stub.sh with DATABASE_URL, then exits.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
PORT=$((20000 + RANDOM % 20000))
"$HERE/node_modules/.bin/pglite-server" -p "$PORT" --include-database-url --run "$HERE/migrate_stub.sh $1" 2>&1 \
  | grep -E '^[0-9]|rror' || true
