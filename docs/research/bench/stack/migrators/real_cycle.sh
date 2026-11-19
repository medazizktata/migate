#!/bin/bash
# A real migrator (node-pg-migrate, Node) applying the 100-table schema to a throwaway native PG 18 (APFS clone).
set -euo pipefail
BIN=/opt/homebrew/opt/postgresql@18/bin
HERE=$(cd "$(dirname "$0")" && pwd)
D=$(mktemp -d "$HERE/../pg/runs/real.XXXXXX")
/bin/cp -c -R "$HERE/../pg/template" "$D/data"
SOCK=$(mktemp -d /tmp/pgs.XXXX)
"$BIN/pg_ctl" -D "$D/data" -w -s -l "$D/log" -o "-c listen_addresses='' -k $SOCK -c fsync=off" start
T0=$(python3 -I -c 'import time; print(time.time())')
DATABASE_URL="postgresql://postgres@localhost/postgres?host=$SOCK" "$HERE/node_modules/.bin/node-pg-migrate" up -m "$HERE/migrations" --no-check-order >/dev/null
T1=$(python3 -I -c 'import time; print(time.time())')
"$BIN/psql" -h "$SOCK" -U postgres -At -F, -f "$HERE/../pg/counts.sql" postgres
echo "migrate_ms=$(python3 -I -c "print(round(($T1-$T0)*1000))")" >&2
"$BIN/pg_ctl" -D "$D/data" -s -m immediate stop
