# Docker sandbox for the headless veloren-agent.
# Builds and tests the agent in an isolated container (no host Rust toolchain needed).
#
# Multi-stage build:
#   builder  - Rust toolchain; fetches + patches dependencies; builds + tests
#   runner   - debian:bookworm-slim carrying only the release binary
###############################################################################
FROM rust:1.82 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install nightly

WORKDIR /src
# CARGO_HOME lives inside the repo workspace so dependency patching and builds
# no longer depend on the host /usr/local/cargo (git checkouts / registry) path.
ENV CARGO_HOME=/src/.cargo
COPY . .

# veloren-client's transitive deps ship code that will not compile on
# modern toolchains out of the box (shred's removed core::ptr::invalid_mut;
# veloren weekly's env!("CARGO_WORKSPACE_DIR"), SendDispatcher API and
# specs::SystemData import gaps). The project's local toolchain-compatibility
# patches live in patches/veloren-weekly-toolchain.patch and are applied to the
# cargo-fetched veloren checkout below — its directory is located dynamically
# because the weekly tag's rev is not fixed (hardcoding
# /usr/local/cargo/git/checkouts/veloren-*/<rev> breaks the moment the tag
# advances). These are the same patches the host build relies on
# (see docs/troubleshooting.md). Then build/test with zero warnings
# on nightly, the project's canonical toolchain.
# shred-0.15 removes core::ptr::invalid_mut; the sed below replaces the call site. It matches loosely
# because the exact line's whitespace/type-args differ across registry revisions, and an unmatched sed
# exits 0 silently, so we grep afterwards to fail the build if the patch did not apply.
# CARGO_HOME lives inside the workspace (/src/.cargo), so patching no longer
# depends on the host /usr/local/cargo/git/checkout path (same approach as the
# .github/workflows/ci.yml nightly job). The apply is idempotent: forward-apply
# when the patch still applies, skip when already applied (warm cache re-runs),
# fail only on genuine conflict or upstream source drift.
RUN cargo fetch \
  && VELOREN_SRC="$(find "$CARGO_HOME/git/checkouts" -maxdepth 8 -type f -path '*/common/base/src/userdata_dir.rs' -print -quit)" \
  && if [ -z "$VELOREN_SRC" ]; then echo "veloren weekly checkout not found after cargo fetch" >&2; exit 1; fi \
  && VELOREN_ROOT="$(cd "$(dirname "$VELOREN_SRC")/../../.." && pwd)" \
  && PATCH="/src/patches/veloren-weekly-toolchain.patch" \
  && if (cd "$VELOREN_ROOT" && git apply --check "$PATCH"); then \
       (cd "$VELOREN_ROOT" && git apply "$PATCH"); \
     elif (cd "$VELOREN_ROOT" && git apply --reverse --check "$PATCH"); then \
       echo "veloren-weekly toolchain patch already applied — skipping"; \
     else \
       echo "veloren-weekly toolchain patch does not apply cleanly" >&2; exit 1; \
     fi \
  && SHRED_META="$(find "$CARGO_HOME/registry/src" -type f -path '*/shred-0.15.0/src/meta.rs' -print -quit)" \
  && if [ -z "$SHRED_META" ]; then echo "shred-0.15.0/src/meta.rs not found in cargo registry" >&2; exit 1; fi \
  && sed -i 's|core::ptr::invalid_mut::<R>((self as \\*mut Self).addr())|(self as *mut Self).wrapping_add(1).cast::<R>()|' "$SHRED_META" \
  && if grep -q 'core::ptr::invalid_mut' "$SHRED_META"; then \
       echo "shred-0.15.0 toolchain patch did not apply: invalid_mut still present" >&2; exit 1; \
     fi

# Build the agent and run the test suite inside the sandbox with zero warnings.
RUN RUSTFLAGS="-D warnings" cargo +nightly build --release
RUN RUSTFLAGS="-D warnings" cargo +nightly test --release

###############################################################################
FROM debian:bookworm-slim AS runner

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/veloren
COPY --from=builder /src/target/release/veloren-agent /usr/local/bin/veloren-agent

ENTRYPOINT ["/usr/local/bin/veloren-agent"]
CMD ["--server", "veloren-server:14004"]
