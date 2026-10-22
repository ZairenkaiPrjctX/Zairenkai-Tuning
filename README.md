# Zairenkai Tuning (`ztune`)

The GPL-3.0 component of Zairenkai: telemetry and validated tuning for the `zairenkai` root
module. It is a separate program; Zairenkai-Core runs it and reads a versioned JSON contract
([docs/CONTRACT.md](docs/CONTRACT.md)). The rest of Zairenkai stays Apache-2.0
([docs/LICENSING.md](docs/LICENSING.md)).

## What it does

| Area | Commands | Adapted from |
|---|---|---|
| Telemetry: CPU frequency, governor and load; CPU/GPU temperature; memory and swap; GPU devfreq; storage schedulers; SoC vendor | `ztune telemetry`, `ztune capabilities` | RvSystem-Monitor (collectors), Lynx (vendor detection) |
| Tuning: block I/O scheduler, GPU devfreq governor | `ztune apply <op> <value> [--dry-run]`, `ztune restore` | Lynx (scheduler and governor selection, verified writer, transactional executor) |
| GPU frequency table in a device tree source: inspect, cap the maximum frequency | `ztune gpu-table inspect\|plan\|stage` | KonaBess (chip identification, table parser and editor) |

Every reading has a status and a unit; a failed read is never replaced by a number. A tuning
value must be one the device advertises; the original value is journalled before the first
write; every write is read back; on failure everything written is rolled back. Nothing runs at
install or boot. The GPU table tool edits files only and never flashes
([docs/GPU_TABLE.md](docs/GPU_TABLE.md)).

Not taken from the upstreams, on purpose: thermal bypass, CPU/GPU frequency locking,
undervolt/overclock kernel patches, charging control, boot image flashing
([docs/PROVENANCE.md](docs/PROVENANCE.md)).

## Build and test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
ANDROID_NDK_HOME=<ndk r27d> scripts/build-android.sh   # -> dist/zairenkai-tuning/
```

The Android build packages the binary with its licence, notices and the Corresponding
Source (the commit's tree plus every linked crate, vendored), for Zairenkai-Modules.

## Status

Tested on a synthetic fixture tree only. Nothing has run on an Android device.

## Licence

GPL-3.0-only ([LICENSE](LICENSE), [NOTICE](NOTICE)). Upstream copyright and attribution:
[NOTICE](NOTICE) and [docs/PROVENANCE.md](docs/PROVENANCE.md). Rust crates:
[docs/THIRD_PARTY.md](docs/THIRD_PARTY.md).
