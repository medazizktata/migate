#!/bin/bash
# Stands in for a user's migrate command: reads DATABASE_URL like prisma/payload would. $1 = start | full.
set -euo pipefail
PSQL=/opt/homebrew/opt/postgresql@18/bin/psql
HERE=$(cd "$(dirname "$0")" && pwd)
: "${DATABASE_URL:?DATABASE_URL not set}"
if [ "$1" = full ]; then
  "$PSQL" "$DATABASE_URL" -q -v ON_ERROR_STOP=1 -f "$HERE/../schema_pg.sql"
  "$PSQL" "$DATABASE_URL" -At -F, -f "$HERE/counts.sql"
else
  "$PSQL" "$DATABASE_URL" -At -c 'select 1'
fi
