# Architecture: Zairenkai-Tuning

> Owned by this repository. Document only what inspected code supports, citing file paths.

## Overview

`ztune` prints one JSON document per run (`docs/CONTRACT.md`): identity, capabilities,
telemetry, the result of a tuning operation, a restore, or a GPU table inspection/plan/stage.
Zairenkai-Core runs it as root; nothing else in Zairenkai links it.

## Components

| Path | Responsibility |
|---|---|
| `crates/ztune-core/src/contract.rs` | Envelope, readings, capability states, operation results, error codes |
| `crates/ztune-core/src/{cpu,thermal,memory}.rs` | Parsers adapted from RvSystem-Monitor |
| `crates/ztune-core/src/{platform,blockdev,gpu}.rs` | Vendor detection and target rules adapted from Lynx |
| `crates/ztune-core/src/ops.rs` | The two operations and their validation |
| `crates/ztune-core/src/gpu_table/` | KonaBess chip identification, table parser and cap edit |
| `crates/ztune/src/sysroot.rs` | All device I/O, rooted for fixtures |
| `crates/ztune/src/collect.rs` | Telemetry, targets, capabilities |
| `crates/ztune/src/{writer,journal,apply}.rs` | Verified writes, boot-scoped journal and lock, apply/rollback/restore |
| `crates/ztune/src/gputable.rs` | Device match, backup, confirmation token, staging |
| `crates/ztune/src/cli.rs`, `main.rs` | Arguments, dispatch, exit codes |

## Data flow and trust boundaries

Input: sysfs/procfs through `SysRoot`, a device tree source file for `gpu-table`. Output:
stdout JSON; writes to the targets of one operation; `/data/adb/zairenkai/tuning/{journal.json,lock}`;
`gpu-table stage` files in a new directory. Runs as root when started by Core; the binary is
root-owned in the module directory. No network, no shell.

## Generated artifacts

`dist/zairenkai-tuning/` from `scripts/build-android.sh` (binary, notices, source archive,
`component.json`). Never committed.
