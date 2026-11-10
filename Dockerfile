# syntax=docker/dockerfile:1
# Runs the vacuity checker on untrusted Verus code inside a container.
# See docker/README.md. Build:  docker build -t vacuity-checker .
# Verus's Linux binaries need glibc >= 2.39, hence trixie (bookworm has 2.36).
# Must match the Rust toolchain the chosen Verus release was built against
# (see `toolchain` in version.json inside the Verus release zip).
ARG RUST_VERSION=1.99.1

# ---- stage 1: build the checker ------------------------------------------------
FROM rust:${RUST_VERSION}-trixie AS build
WORKDIR /src
COPY checker/ checker/
RUN cd checker && cargo build --release

# ---- stage 2: runtime ------------------------------------------------------------
FROM rust:${RUST_VERSION}-trixie
ARG TARGETARCH
ARG VERUS_VERSION=0.2026.11.02.8bf68ee

RUN apt-get update \
 && apt-get install -y --no-install-recommends unzip curl ca-certificates git \
 && rm -rf /var/lib/apt/lists/*

# Official Verus release for this CPU (arm64 on Apple Silicon, x86 elsewhere).
RUN set -eux; \
    case "$TARGETARCH" in arm64) a=arm64;; amd64) a=x86;; *) echo "unsupported arch $TARGETARCH" >&2; exit 1;; esac; \
    curl -fsSL -o /tmp/verus.zip \
      "https://github.com/verus-lang/verus/releases/download/release/${VERUS_VERSION}/verus-${VERUS_VERSION}-${a}-linux.zip"; \
    mkdir /tmp/verus-unzip; unzip -q /tmp/verus.zip -d /tmp/verus-unzip; \
    d="$(dirname "$(find /tmp/verus-unzip -name verus -type f | head -n1)")"; \
    mv "$d" /opt/verus; \
    rm -rf /tmp/verus.zip /tmp/verus-unzip; \
    /opt/verus/verus --version

COPY --from=build /src/checker/target/release/vacuity-checker /usr/local/bin/vacuity-checker
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh

# Unprivileged user; the toolchain stays in /usr/local/rustup, cargo's cache moves to the user's home.
RUN useradd --create-home runner \
 && mkdir /work /output && chown runner:runner /work /output \
 && git config --system --add safe.directory '*'
USER runner
ENV VERUS_PATH=/opt/verus/verus \
    CARGO_HOME=/home/runner/.cargo
WORKDIR /work
ENTRYPOINT ["entrypoint.sh"]
