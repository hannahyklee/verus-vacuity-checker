# Running the checker in Docker

Use this for repositories you don't fully trust. Verus runs proc macros and `build.rs`
scripts, and the checker rewrites source files in place, so a repo should not run
directly on your machine.

```sh
scripts/docker-sweep.sh <repo_dir> [subdir] [--jobs N] [--timeout SECS]
# e.g.
scripts/docker-sweep.sh experiments/verus-src examples --jobs 2
```

Results land in `results/docker/<repo name>/report/` (`report.md`, `summary.json`, one
`.diff` per mutant).

## What is isolated

- The repo is mounted **read-only** at `/input`. The container copies it to `/work` and
  mutates only the copy, so your checkout is never modified (no crash can leave a mutated
  file behind).
- Non-root user, all Linux capabilities dropped, `no-new-privileges`, CPU/memory/pid limits
  (`CPUS`, `MEMORY` env vars).
- Only `/output` is writable on the host.

## What is not isolated

- **Network is on by default**, because crates fetch dependencies (e.g. `vstd`) from
  crates.io at build time. Code from the repo can therefore reach the network while it
  builds. For standalone `.rs` files (no `Cargo.toml`) use `DOCKER_NETWORK=none`.
- The container shares your kernel, so this is not a defence against a kernel exploit.
  For hostile code use a VM.

## Notes

- Tested on Apple Silicon (arm64 Linux, native, no emulation). On a Mac, Docker Desktop's
  `docker` binary may not be on your `PATH`; add `~/.docker/bin` or
  `/Applications/Docker.app/Contents/Resources/bin`.
- The base image is Debian trixie because Verus's Linux binaries need glibc >= 2.39.
- Each run copies the whole mounted repo into the container, so mount the repo root and
  pass the subdirectory to check (a 450 MB repo adds a few seconds).
- The report's repository URL and commit come from the mounted repo's `.git`, so mount the
  repo root, not an unrelated subdirectory.

## Versions

`VERUS_VERSION` (Dockerfile build arg, default `0.2026.11.02.8bf68ee`) selects the Verus
release; `RUST_VERSION` must match the toolchain that release needs. Both default to the
versions used on the host. Verus publishes `arm64-linux` and `x86-linux` builds, chosen
automatically from the image platform.
