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
RUN cargo fetch \
  && VELOREN_SRC="$(find /usr/local/cargo/git/checkouts -maxdepth 8 -type f -path '*/common/base/src/userdata_dir.rs' -print -quit)" \
  && if [ -z "$VELOREN_SRC" ]; then echo "veloren weekly checkout not found after cargo fetch" >&2; exit 1; fi \
  && VELOREN_ROOT="$(cd "$(dirname "$VELOREN_SRC")/../../.." && pwd)" \
  && (cd "$VELOREN_ROOT" && git apply /src/patches/veloren-weekly-toolchain.patch) \
  && SHRED_META="$(find /usr/local/cargo/registry/src -type f -path '*/shred-0.15*/src/meta.rs' -print -quit)" \
  && if [ -z "$SHRED_META" ]; then echo "shred registry source not found after cargo fetch" >&2; exit 1; fi \
  && sed -i 's|^[[:space:]]*let invalid_ptr =.*invalid_mut.*|        let invalid_ptr = (self as *mut Self).wrapping_add(1).cast::<R>();|' "$SHRED_META" \
  && grep -q 'wrapping_add(1).cast::<R>()' "$SHRED_META"

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
