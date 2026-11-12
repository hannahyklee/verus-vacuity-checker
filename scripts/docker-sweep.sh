#!/usr/bin/env bash
# Sweep a repository for vacuous specs inside a locked-down container.
#   scripts/docker-sweep.sh <repo_dir> [subdir] [--config configs/<project>.toml] [--jobs N] [--timeout S]
# With --config, exactly the crates listed there are checked, with their cargo flags, and the
# container image is built for the Verus version the config names (see configs/README.md).
# Results: results/docker/<repo name>/report/{report.md,summary.json,...}
#
# Env: DOCKER_PLATFORM (e.g. linux/amd64: run an x86 image under emulation, for Verus releases
#      that have no arm64 Linux build), CPUS (default 4), MEMORY (default 8g), DOCKER_NETWORK (default bridge; crates need
#      network to fetch dependencies, single files do not: use DOCKER_NETWORK=none).
set -euo pipefail

if [ "$#" -lt 1 ]; then
    sed -n '2,10p' "$0" >&2
    exit 2
fi

repo="$(cd "$1" && pwd)"
shift
sub="."
if [ "$#" -gt 0 ] && [[ "$1" != --* ]]; then
    sub="$1"
    shift
fi

# Pull `--config FILE` out of the remaining arguments.
config=""
rest=()
while [ "$#" -gt 0 ]; do
    if [ "$1" = "--config" ]; then
        config="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
        shift 2
    else
        rest+=("$1")
        shift
    fi
done
set -- ${rest[@]+"${rest[@]}"}

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${root}/results/docker/$(basename "$repo")"
mkdir -p "$out"

image="vacuity-checker"
plat=()
if [ -n "${DOCKER_PLATFORM:-}" ]; then
    plat=(--platform "$DOCKER_PLATFORM")
    image="vacuity-checker-${DOCKER_PLATFORM//\//-}"
fi
docker build -q ${plat[@]+"${plat[@]}"} -t "$image" "$root" >/dev/null

config_args=()
mounts=()
if [ -n "$config" ]; then
    # The default image can parse the config; it tells us which Verus the project needs.
    version="$(docker run --rm ${plat[@]+"${plat[@]}"} -v "${config}:/config.toml:ro" --entrypoint vacuity-checker "$image" config-version /config.toml)"
    if [ -n "$version" ]; then
        image="${image}:verus-${version}"
        docker build -q ${plat[@]+"${plat[@]}"} --build-arg "VERUS_VERSION=${version}" -t "$image" "$root" >/dev/null
    fi
    mounts=(-v "${config}:/config.toml:ro")
    config_args=(--config /config.toml)
fi

status=0
docker run --rm ${plat[@]+"${plat[@]}"} \
    -e VACUITY_ROOT_LABEL="${repo}/${sub}" \
    --network "${DOCKER_NETWORK:-bridge}" \
    --cpus "${CPUS:-4}" --memory "${MEMORY:-8g}" --pids-limit 1024 \
    --cap-drop ALL --security-opt no-new-privileges \
    -v "${repo}:/input:ro" \
    -v "${out}:/output" \
    ${mounts[@]+"${mounts[@]}"} \
    "$image" "$sub" ${config_args[@]+"${config_args[@]}"} "$@" || status=$?
echo "report: ${out}/report/report.md"
exit "$status"
