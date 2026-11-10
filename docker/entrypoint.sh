#!/usr/bin/env bash
# Container entrypoint: copy the read-only input to a scratch dir, then sweep it.
#   entrypoint.sh <path-inside-repo> [vacuity-checker sweep options...]
# The checker mutates source files in place, so it only ever touches the copy in
# /work; /input is mounted read-only and results go to /output.
set -euo pipefail

sub="${1:-.}"
[ "$#" -gt 0 ] && shift

if [ ! -d /input ]; then
    echo "error: mount the repository at /input (read-only)" >&2
    exit 2
fi

cp -a /input/. /work/

target="/work/${sub}"
if [ ! -d "$target" ]; then
    echo "error: '${sub}' is not a directory inside the repository" >&2
    exit 2
fi

exec vacuity-checker sweep "$target" --out /output/report "$@"
