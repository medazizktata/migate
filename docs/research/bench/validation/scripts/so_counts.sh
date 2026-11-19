#!/bin/zsh
# Stack Overflow counts per tag x theme sub-query (title= is substring on title; q= is site full-text).
out=${OUT:-./out/$(basename $0 .sh)}; mkdir -p $out
F='!SyjNqbwGU2NWZ210M0'
tags=(django-migrations entity-framework-migrations prisma alembic flyway liquibase rails-migrations)
subs=("a|title|checksum" "a|q|\"modified migration\"" "b|title|out of order" "b|q|\"conflicting migrations\"" "b|q|\"multiple heads\"" "b|title|merge" "c|title|drift" "c|title|out of sync" "c|q|\"migration history\"" "d|title|production" "d|title|data loss" "d|title|downtime")
print -r -- "# $(date -u +%FT%TZ)" > $out/summary.tsv
for t in $tags; do for s in $subs; do
  th=${s%%|*}; rest=${s#*|}; p=${rest%%|*}; v=${rest#*|}
  f=$out/${th}__${t}__${p}_${${v//[^a-zA-Z]/_}}.json
  curl -s --compressed -G "https://api.stackexchange.com/2.3/search/advanced" --data-urlencode "site=stackoverflow" --data-urlencode "tagged=$t" --data-urlencode "$p=$v" --data-urlencode "sort=votes" --data-urlencode "pagesize=3" --data-urlencode "filter=$F" -o $f
  print -r -- "$th	$t	$p=$v	$(jq -r '.total' $f)	$(jq -r '[.items[]|"\(.score)|\(.link)|\(.title)"]|join(" ;; ")' $f)" >> $out/summary.tsv
  b=$(jq -r '.backoff // 0' $f); sleep $(( b + 1 ))
done; done
jq -r .quota_remaining $f
