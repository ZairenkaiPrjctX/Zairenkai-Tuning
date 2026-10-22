# CLAUDE.md: Zairenkai-Tuning

Instructions for Claude Code in `ZairenkaiPrjctX/Zairenkai-Tuning`. Keep this file under 200 lines;
put detail in `AGENT/`.

@AGENTS.md
@AGENT/PROJECT_CONTEXT.md

## Current state (read first)

This repository is **Zairenkai Tuning**, the organization's **GPL-3.0-only** component: a Rust
workspace that builds `ztune`, a command-line program for telemetry and validated tuning.
- `crates/ztune-core`: the contract (`zairenkai.tuning` schema 1), parsers, validation,
  SoC family registry (`data/soc-registry.json`), device identity, profiles and the tier
  resolver, the GPU table editor. No I/O.
- `crates/ztune`: SysRoot reads, verified writes, the restore journal, the CLI.
- `scripts/build-android.sh`: the component for Zairenkai-Modules, with its Corresponding Source.

It adapts code from RvSystem-Monitor, Lynx and KonaBess (`docs/PROVENANCE.md`). Contract:
`docs/CONTRACT.md`. Licence boundary: `docs/LICENSING.md`. Nothing has run on a device.

Non-negotiables in this repo:
- **Licence.** Every file is GPL-3.0-only. Adapted files keep the upstream copyright line,
  upstream file and commit, and a dated list of changes. Never relicense a file, and never move
  code from here (or from the upstreams) into an Apache-2.0 repository.
- **Provenance first.** Before adapting more upstream code, record the URL, commit, licence,
  authors and files in `docs/PROVENANCE.md`.
- **Writes.** Only the operations in `ztune_core::ops` (`io.scheduler`, `gpu.governor`), only
  values the device advertises **and** a Tier D (exact device/build, `device_validated`,
  hardware-tested) profile grants (`docs/PROFILES.md`, `docs/adr/0001-write-path-tiers.md`);
  validated before the lock, journalled before the first write, read back, rolled back on
  failure. A family match never authorizes a write. Never ship a synthetic profile. No chmod, no thermal nodes, no CPU/GPU frequency limits,
  no vendor configuration, no writes at install or boot.
- **Readings** carry a status and a unit; never substitute a number for a failed read.
- **GPU table:** underclock edits of a device tree file only, behind device match, backup and
  a confirmation token. Never dump, repack or flash a partition (`docs/GPU_TABLE.md`).
- **Corresponding Source** ships with every binary (`scripts/build-android.sh`). Never commit
  binaries or prebuilt tools (`magiskboot`, `dtc`, ...).
- Tests use the fixture root (`crates/ztune/tests/fixtures`), never the host's `/sys`, `/proc`.

## Commands

| Purpose | Command | Status |
|---|---|---|
| Build | `cargo build --workspace` | Verified. Evidence: 2026-10-22 |
| Test | `cargo test --workspace` | Verified. Evidence: 68 passed (44 core + 13 unit + 11 CLI), 2026-10-22 |
| Lint / format | `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check` | Verified. Evidence: clean, 2026-10-22 |
| MSRV | `cargo +1.85.0 test --workspace` | Verified. Evidence: passed, 2026-10-22 |
| Shell lint | `shellcheck scripts/*.sh` | Verified. Evidence: clean (0.11.0), 2026-10-22 |
| SoC registry | `scripts/update-soc-registry.py <linux checkout>` | Verified. Evidence: 443 identifiers from Linux 7be4bc52, 2026-10-22 |
| Android component | `ANDROID_NDK_HOME=<ndk> scripts/build-android.sh` | Verified. Evidence: NDK r27d (SHA-256 checked), aarch64 PIE for `/system/bin/linker64`, 16 KB LOAD alignment checked; the source archive rebuilt the binary offline with an empty `CARGO_HOME`, 2026-10-22 |
| Validate agent foundation | `python3 <path-to-.github>/agent-foundation/scripts/validate_agent_setup.py .` | Configured |

Only list commands that exist in this repository. Never invent one.

## Working rules

- Inspect the relevant code before editing it. Preserve unrelated and uncommitted changes.
- Generated files are edited at their source, then regenerated. Managed foundation files
  (listed in `AGENT/.foundation-manifest.json`) are edited in `ZairenkaiPrjctX/.github`, not here.
- Destructive or outward-facing actions (deleting data, force-push, history rewrites,
  publishing releases, deploying, flashing devices) need explicit human approval each time.
- Do not claim a build, test, or signature succeeded without showing the output.

## Git

- Work on a topic branch; open **draft** pull requests; never merge or approve.
- Commits must be cryptographically signed. If signing fails, stop and report the error.
  Trailer format and verification: `AGENT/policies/git-and-commit-signing.md`.

## Where things are

- `AGENT/README.md`: index of the agent library (prompts, policies, playbooks, templates).
- `.claude/skills/`: run `/repo-audit`, `/implement-feature`, `/bug-fix`, `/test-and-verify`,
  `/security-review`, `/release-readiness`.
- `.claude/rules/`: path-scoped rules, loaded automatically.
