# Docker sandbox for the headless veloren-agent.
# Builds and tests the agent in an isolated container (no host Rust toolchain needed).
FROM rust:1.99

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
# patches live in patches/veloren-weekly-toolchain.patch (git checkouts) and
# are applied to the fetched sources below — the same patches the host build
# relies on (see docs/troubleshooting.md). Then build/test with zero warnings
# on nightly, the project's canonical toolchain.
RUN cargo fetch
RUN cd /usr/local/cargo/git/checkouts/veloren-*/f97be34 && git apply /src/patches/veloren-weekly-toolchain.patch
RUN sed -i 's|let invalid_ptr = core::ptr::invalid_mut::<R>((self as \*mut Self).addr());|let invalid_ptr = (self as *mut Self).wrapping_add(1).cast::<R>();|' /usr/local/cargo/registry/src/*/shred-0.15.0/src/meta.rs

# Build the agent and run the test suite inside the sandbox with zero warnings.
RUN RUSTFLAGS="-D warnings" cargo +nightly build --release
RUN RUSTFLAGS="-D warnings" cargo +nightly test --release

CMD ["cargo", "+nightly", "build", "--release"]
