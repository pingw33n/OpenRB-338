#!/usr/bin/env bash
# Clone one of the collection repositories (rb-songs or rb-mods of pingw33n) shallow, at the
# given commit, into DEST/NAME (task-322). The Pages workflow uses it on a cache miss only.
#
#   scripts/clone-collection.sh NAME COMMIT DEST
#
# A shallow fetch of one commit by its hash (GitHub allows it), so a push after the commit
# was read with `git ls-remote` does not change what the index pins.
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "usage: $0 NAME COMMIT DEST" >&2
    exit 2
fi
name=$1 commit=$2 dest=$3

mkdir -p "$dest/$name"
cd "$dest/$name"
git init --quiet
git remote add origin "https://github.com/pingw33n/$name.git"
git fetch --quiet --depth 1 origin "$commit"
git checkout --quiet FETCH_HEAD
got=$(git rev-parse HEAD)
[ "$got" = "$commit" ] || { echo "::error::$name is at $got, not $commit" >&2; exit 1; }
echo "$name cloned at $got"
