#!/usr/bin/env bash
# Sweep a repository for vacuous specs inside a locked-down container.
#   scripts/docker-sweep.sh <repo_dir> [subdir] [sweep options, e.g. --jobs 2 --timeout 300]
# Results: results/docker/<repo name>/report/{report.md,summary.json,...}
#
# Env: CPUS (default 4), MEMORY (default 8g), DOCKER_NETWORK (default bridge; crates need
#      network to fetch dependencies, single files do not: use DOCKER_NETWORK=none).
set -euo pipefail

if [ "$#" -lt 1 ]; then
    sed -n '2,8p' "$0" >&2
    exit 2
fi

repo="$(cd "$1" && pwd)"
shift
sub="."
if [ "$#" -gt 0 ] && [[ "$1" != --* ]]; then
    sub="$1"
    shift
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${root}/results/docker/$(basename "$repo")"
mkdir -p "$out"

docker build -q -t vacuity-checker "$root" >/dev/null

status=0
docker run --rm \
    -e VACUITY_ROOT_LABEL="${repo}/${sub}" \
    --network "${DOCKER_NETWORK:-bridge}" \
    --cpus "${CPUS:-4}" --memory "${MEMORY:-8g}" --pids-limit 1024 \
    --cap-drop ALL --security-opt no-new-privileges \
    -v "${repo}:/input:ro" \
    -v "${out}:/output" \
    vacuity-checker "$sub" "$@" || status=$?
echo "report: ${out}/report/report.md"
exit "$status"
