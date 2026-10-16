# Build Troubleshooting

Fixing Rust build issues on the stable toolchain for the Veloren agent.

## Problem

`cargo build` fails with errors like:

```
error[E0554]: `#![feature]` may not be used on the stable release channel
  --> foldhash-0.2.0/src/lib.rs: ... feature(hasher_prefixfree_extras)
error[E0658]: use of unstable library feature `allocator_api`
  --> hashbrown-0.17.1/src/table.rs: ...
error[E0425]: cannot find function `invalid_mut` in module `core::ptr`
  --> shred-0.15.0/src/meta.rs: ...
```

## Cause

`veloren-client` (from Veloren's gitlab `weekly` tag) pulls in `hashbrown`, `shred`, and `foldhash` with `nightly` features enabled via `specs` feature unification. These crates use unstable Rust features unavailable on the stable toolchain.

## Fix: use nightly Rust

```bash
rustup toolchain install nightly
cargo +nightly build --release
```

## Fix: patch registry crates (local only)

Remove nightly feature gates from the cargo registry crates. These patches are local to your machine and are **not** committed to the repo.

### foldhash

File: `~/.cargo/registry/src/*/foldhash-0.2.0/src/lib.rs`
Remove: `#![cfg_attr(feature = "nightly", feature(hasher_prefixfree_extras))]`

Files: `fast.rs`, `quality.rs`
Remove the `#[cfg(feature = "nightly")] fn write_str(...)` blocks.

### hashbrown

File: `~/.cargo/registry/src/*/hashbrown-0.17.1/src/lib.rs`
Remove the `#![cfg_attr(feature = "nightly", feature(...))]` block containing `core_intrinsics`, `dropck_eyepatch`, `min_specialization`, `trivial_clone`, `extend_one`, `allocator_api`, `strict_provenance_lints`.

### shred

File: `~/.cargo/registry/src/*/shred-0.15.0/src/lib.rs`
Remove: `#![cfg_attr(feature = "nightly", feature(ptr_metadata, strict_provenance))]`

## Fix: write the agent in Python

Raw TCP to `127.0.0.1:14004` works fine. A Python client using the Veloren network protocol directly avoids the Rust build issues entirely. This is a candidate for a future `python/` directory.

## Still failing?

Open an issue with:
- Full error output
- `rustc --version`
- Whether Airshipper-installed Veloren is being used
