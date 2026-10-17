# Docker sandbox for the headless veloren-agent.
# Builds and tests the agent in an isolated container (no host Rust toolchain needed).
FROM rust:1.99

RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install nightly

WORKDIR /src
COPY . .

# veloren-client's transitive deps ship nightly-only code that will not
# compile on modern toolchains: shred-0.15.0 calls core::ptr::invalid_mut,
# which no longer exists. Patch the fetched registry sources (the same
# local patches the project relies on, see docs/troubleshooting.md),
# then build/test with zero warnings on nightly.
RUN cargo fetch
RUN sed -i 's|let invalid_ptr = core::ptr::invalid_mut::<R>((self as \*mut Self).addr());|let invalid_ptr = (self as *mut Self).wrapping_add(1).cast::<R>();|' /usr/local/cargo/registry/src/*/shred-0.15.0/src/meta.rs
# veloren-common-base (git dep) hardcodes env!("CARGO_WORKSPACE_DIR"), unavailable
# for git dependencies; fall back at runtime (same local patch as the host checkout).
RUN sed -i 's#env!("CARGO_WORKSPACE_DIR")#std::env::var("CARGO_WORKSPACE_DIR").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned())#' /usr/local/cargo/git/checkouts/veloren-*/f97be34/common/base/src/userdata_dir.rs

# Build the agent and run the test suite inside the sandbox with zero warnings.
RUN RUSTFLAGS="-D warnings" cargo +nightly build --release
RUN RUSTFLAGS="-D warnings" cargo +nightly test --release

CMD ["cargo", "+nightly", "build", "--release"]
