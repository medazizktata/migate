#!/bin/bash
# One throwaway Postgres 18 container cycle from the locally present image (no pulls). $1 = start | full.
# Uses tmpfs for PGDATA and fsync=off, as a CI service container would be tuned. Container auto-removes on stop.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
PSQL=/opt/homebrew/opt/postgresql@18/bin/psql
PORT=$((20000 + RANDOM % 20000))
CID=$(docker run -d --pull=never --rm -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:$PORT:5432 \
  --tmpfs /var/lib/postgresql postgres:18 -c fsync=off)
URL="postgresql://postgres@127.0.0.1:$PORT/postgres"
for i in $(seq 1 600); do "$PSQL" "$URL" -Atqc 'select 1' >/dev/null 2>&1 && break; sleep 0.02; done
DATABASE_URL=$URL "$HERE/migrate_stub.sh" "$1"
docker kill "$CID" >/dev/null
