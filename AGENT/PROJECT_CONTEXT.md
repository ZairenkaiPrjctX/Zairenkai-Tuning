# Project context: Zairenkai-Tuning

> Owned by this repository. Update it whenever verified facts change. Label every statement
> **Verified** (with `Evidence:`), **Unverified**, or **Unknown / Not Configured**.

## Identity

| Field | Value | Status |
|---|---|---|
| Repository | `ZairenkaiPrjctX/Zairenkai-Tuning` | Verified. Evidence: `git remote -v`, session scope, 2026-10-22 |
| Remote state | Empty before this change (no branches); no default branch exists yet | Verified. Evidence: `git ls-remote origin` returned no refs, 2026-10-22 |
| Purpose | GPL-3.0 component of Zairenkai: telemetry and validated tuning (`ztune`), adapted from RvSystem-Monitor, Lynx and KonaBess | Verified. Evidence: `crates/`, `docs/PROVENANCE.md` |
| Name | The repository keeps its name; the component is `zairenkai-tuning`, the binary `ztune`. A rename to Zairenkai-GPL-Components would be an organization decision | Verified. Evidence: maintainer brief 2026-10-22 allowed either name |
| Licence | GPL-3.0-only (`LICENSE`, `NOTICE`, SPDX headers, Cargo `license`) | Verified. Evidence: maintainer decision 2026-10-22 ("komponen GPL terpisah") |

## Contents

| Area | Location | Status |
|---|---|---|
| Rust workspace (MSRV 1.85; serde, serde_json, sha2) | `Cargo.toml`, `Cargo.lock` | Verified. Evidence: `cargo +1.85.0 test --workspace`, 2026-10-22 |
| Contract, parsers, validation, GPU table (no I/O) | `crates/ztune-core` | Verified. Evidence: 33 tests, 2026-10-22 |
| SysRoot I/O, writer, journal, apply/restore, CLI | `crates/ztune` | Verified on fixtures. Evidence: 10 unit + 8 CLI tests, 2026-10-22. **Unverified** on a device |
| Android component and Corresponding Source | `scripts/build-android.sh` | Verified. Evidence: NDK r27d build, exit 0; `cargo build --offline` from the extracted source archive succeeded with an empty `CARGO_HOME`, 2026-10-22. The rebuilt binary is not byte-identical (build paths are embedded) |
| CI | `.github/workflows/ci.yml` | **Unverified** on GitHub |

## Key facts and limitations

- Upstreams audited at RvSystem-Monitor `b07fe27`, Lynx `e56d6cd`, KonaBess `ff387dc`; each is
  GPL-3.0 without an "or later" clause and without per-file notices. Verified. Evidence:
  `docs/PROVENANCE.md`, 2026-10-22.
- Writes: `io.scheduler` and `gpu.governor` only, values the device advertises. Verified.
  Evidence: `ztune_core::ops`, `apply.rs` tests.
- GPU table: inspect/plan/stage on device tree source only; flashing is not implemented.
  Verified. Evidence: `gputable.rs`, CLI test `gpu_table_plan_needs_matching_device_and_token`.
- No Android device was available: real sysfs layouts, SELinux denials and kernel read-back
  behaviour are **Unverified**.

## Authoritative sources

| Topic | Source |
|---|---|
| Repository facts | this file |
| Architecture | `AGENT/ARCHITECTURE.md` |
| Contract | `docs/CONTRACT.md` |
| Provenance and licensing | `docs/PROVENANCE.md`, `docs/LICENSING.md`, `NOTICE` |
| Organization standards | `ZairenkaiPrjctX/.github` → `agent-foundation/standards/` |

## Related repositories

- `Zairenkai-Core`: runs `ztune` through its tuning adapter (topic branch `ccr-63f8aa87-1b0paj`).
- `Zairenkai-Modules`: packages `ztune` and its source into the single `zairenkai` module.
- `Zairenkai-App`: reads tuning capabilities through Core and parses the contract.

## Open questions

- Which device runs the first validation of `io.scheduler` and `gpu.governor`?
- Should the licence be GPL-3.0-or-later (allowed by all three upstreams)?
- Should the repository be renamed, and who creates its `dev` default branch?
