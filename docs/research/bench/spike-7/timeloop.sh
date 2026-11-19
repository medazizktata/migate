#!/bin/bash
# usage: timeloop.sh <repo> <base> <head> <n> <reset> <migrate> <outfile>
repo=$1 base=$2 head=$3 n=$4 reset=$5 mig=$6 out=$7; cd "$repo"; : > "$out"
t() { local a b rc; a=$(python3 -c 'import time;print(time.time())'); bash -c "$mig" >/dev/null 2>&1; rc=$?; b=$(python3 -c 'import time;print(time.time())'); echo "$1 $rc $(python3 -c "print(round($b-$a,3))")" >> "$out"; }
for i in $(seq $n); do bash -c "$reset"; git checkout -q "$base"; t base; git checkout -q "$head"; t head; t nopending; done
git checkout -q "$base"
python3 - "$out" <<'PY'
import sys,statistics as s
rows=[l.split() for l in open(sys.argv[1])]
for k in ('base','head','nopending'):
    v=[float(r[2]) for r in rows if r[0]==k]; rc=set(r[1] for r in rows if r[0]==k)
    print(f"{k:10s} n={len(v)} exits={sorted(rc)} mean={s.mean(v):.2f}s sd={s.pstdev(v):.2f} min={min(v):.2f} max={max(v):.2f}")
PY
