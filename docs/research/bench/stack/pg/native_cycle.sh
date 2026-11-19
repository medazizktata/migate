#!/bin/bash
# One throwaway native Postgres 18 cycle. $1 = initdb (fresh cluster) | clone (APFS clone of a template cluster).
# $2 = start (select 1 only) | full (apply 100-table schema + counts). Data dirs are left behind (no deletes on this machine).
set -euo pipefail
BIN=/opt/homebrew/opt/postgresql@18/bin
HERE=$(cd "$(dirname "$0")" && pwd)
D=$(mktemp -d "$HERE/runs/pg.XXXXXX")
if [ "$1" = initdb ]; then
  "$BIN/initdb" -D "$D/data" -A trust -U postgres --no-sync -E UTF8 --locale=C >/dev/null
else
  /bin/cp -c -R "$HERE/template" "$D/data"
fi
SOCK=$(mktemp -d /tmp/pgs.XXXX)   # short path: unix socket paths are limited to ~104 bytes
"$BIN/pg_ctl" -D "$D/data" -w -s -l "$D/log" -o "-c listen_addresses='' -k $SOCK -c fsync=off" start
if [ "$2" = full ]; then
  "$BIN/psql" -h "$SOCK" -U postgres -q -v ON_ERROR_STOP=1 -f "$HERE/../schema_pg.sql" postgres
  "$BIN/psql" -h "$SOCK" -U postgres -At -F, -f "$HERE/counts.sql" postgres
else
  "$BIN/psql" -h "$SOCK" -U postgres -At -c 'select 1' postgres
fi
"$BIN/pg_ctl" -D "$D/data" -s -m immediate stop
