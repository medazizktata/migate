#!/bin/bash
# Two checkouts, one sandbox.
# usage: flow.sh <repo> <base-ref> <head-ref> <outdir> <reset-cmd> <migrate-cmd> <snap-cmd>
# reset-cmd makes the sandbox empty; migrate-cmd is run in <repo> via bash -c; snap-cmd prints the schema.
set -uo pipefail
repo=$1 base=$2 head=$3 out=$4 reset=$5 mig=$6 snap=$7
mkdir -p "$out"; cd "$repo" || exit 9
now() { python3 -c 'import time;print(time.time())'; }
run() { local tag=$1 t0 t1 rc; t0=$(now); bash -c "$mig" > "$out/$tag.log" 2>&1; rc=$?; t1=$(now)
  python3 -c "print(f'$tag exit=$rc secs={$t1-$t0:.2f}')" | tee -a "$out/summary.txt"; return $rc; }
: > "$out/summary.txt"
bash -c "$reset" || { echo "reset failed" | tee -a "$out/summary.txt"; exit 9; }
git checkout -q "$base" || exit 9
run base || { echo "BASE FAILED" | tee -a "$out/summary.txt"; exit 3; }
bash -c "$snap" > "$out/A.txt" || exit 9
git checkout -q "$head" || exit 9
run head; hrc=$?
bash -c "$snap" > "$out/B.txt" || exit 9
git checkout -q "$base"
diff "$out/A.txt" "$out/B.txt" > "$out/diff.txt"; d=$?
echo "head_exit=$hrc diff_lines=$(grep -c '^[<>]' "$out/diff.txt")" | tee -a "$out/summary.txt"
cat "$out/diff.txt"
exit $hrc
