#!/bin/zsh
# GitHub issue counts per repo x theme; saves raw JSON (top 30 by reactions) per query.
out=${OUT:-./out/$(basename $0 .sh)}; mkdir -p $out
repos=(prisma/orm drizzle-team/drizzle-orm rails/rails sqlalchemy/alembic flyway/flyway liquibase/liquibase payloadcms/payload typeorm/typeorm knex/knex golang-migrate/migrate)
typeset -A Q
Q[a_edited]='migration ("checksum mismatch" OR "check sum" OR "was modified after it was applied" OR "migration was modified" OR "edited migration" OR "modified migration")'
Q[b_order]='migration ("out of order" OR "out-of-order" OR "conflicting migrations" OR "multiple heads" OR "outOfOrder" OR "migration conflict")'
Q[c_drift]='migration ("drift" OR "out of sync" OR "migration history" OR "applied manually" OR "not in sync" OR "manually applied")'
Q[d_prod]='migration ("downtime" OR "production outage" OR "broke production" OR "data loss" OR "dropped column" OR "deleted data")'
print -r -- "date=$(date -u +%FT%TZ)" > $out/summary.tsv
for k in a_edited b_order c_drift d_prod; do
  for r in $repos; do
    q="repo:$r is:issue ${Q[$k]}"
    f=$out/${k}__${r//\//_}.json
    gh api -X GET search/issues -f q="$q" -f advanced_search=true -f sort=reactions -f order=desc -f per_page=30 > $f 2>&1
    print -r -- "$k\t$r\t$(jq -r .total_count $f)\t$q" >> $out/summary.tsv
    sleep 2.3
  done
done
echo done
