# Docker sandbox for the headless veloren-agent.
# Builds and tests the agent in an isolated container (no host Rust toolchain needed).
FROM rust:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Build the agent and run the test suite inside the sandbox.
RUN cargo build --release
RUN cargo test --release

CMD ["cargo", "build", "--release"]
