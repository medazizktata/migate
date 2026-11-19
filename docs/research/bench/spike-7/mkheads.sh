#!/bin/bash
# usage: mkheads.sh <repo> <dir-for-new-migration-file> <filename> <good-sql> <bad-sql>  (prisma-style: one dir per migration)
set -e; cd "$1"; G="git -c user.email=s@x.invalid -c user.name=s"
mk() { git checkout -q -b "$1" base; mkdir -p "$2"; printf '%s\n' "$4" > "$2/$3"; git add -A; $G commit -qm "$1"; git checkout -q base; }
mk head-good "$2" "$3" "$4"; mk head-bad "$2" "$3" "$5"; mk head-noopmig "$2" "$3" "-- nothing"
git checkout -q -b head-noopcommit base; echo spike >> NOTES.txt; git add NOTES.txt; $G commit -qm noop; git checkout -q base
git log --oneline --all
