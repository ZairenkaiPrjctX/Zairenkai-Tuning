# Third-party Rust crates

Crates in `Cargo.lock` that `ztune` uses for `aarch64-linux-android`, from `cargo metadata`
(2026-10-22). Every licence is compatible with GPL-3.0. The full source of each crate is in
the component's source archive (`vendor/`), with its own licence files.

Linked into the binary:

| Crate | Version | Licence |
|---|---|---|
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| foldhash | 0.2.0 | Zlib |
| generic-array | 0.14.7 | MIT |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| serde, serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| zmij | 1.0.23 | MIT |

Build time only (procedural macros and build scripts, not in the binary): proc-macro2 1.0.107,
quote 1.0.47, syn 3.0.9, serde_derive 1.0.229 (MIT OR Apache-2.0), unicode-ident 1.0.27
((MIT OR Apache-2.0) AND Unicode-3.0), version_check 0.9.5 (MIT/Apache-2.0).

The GPL-3.0 projects code is adapted from are listed in `NOTICE` and `PROVENANCE.md`.
