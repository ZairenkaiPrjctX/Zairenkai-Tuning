# Provenance of adapted code

Zairenkai-Tuning adapts code from three GPL-3.0 projects. Each was audited at one commit on
2026-10-22. Every adapted file keeps the upstream copyright line, names the upstream file and
commit in its header, and lists its changes. This page is the record of what was taken, what
was changed, and what was deliberately not taken.

None of the upstream repositories states "version 3 or (at your option) any later version";
each ships the verbatim GPLv3 text (SHA-256 `3972dc97…986`, identical to
<https://www.gnu.org/licenses/gpl-3.0.txt>) and its README says "GPL v3.0". Under GPLv3
section 14 a program that does not specify a version may be used under any published version;
this component is licensed **GPL-3.0-only** (maintainer decision, 2026-10-22). Changing that to
GPL-3.0-or-later would be compatible with all three upstreams and needs only a maintainer
decision, not upstream consent.

## RvSystem-Monitor

| Field | Value |
|---|---|
| URL | <https://github.com/Rve27/RvSystem-Monitor> |
| Commit | `b07fe27598555e49084bbcc40be558803286b9ef` (2026-10-20) |
| Licence | GPL-3.0 (`LICENSE`, README "GNU General Public License v3.0") |
| Copyright | No per-file notices upstream. Authors of the adapted files by `git log --follow`: Rve27 (Radika, `rve27github@gmail.com`) and Pavelc4. Recorded as "Copyright (C) 2026 Rve27 (Radika), Pavelc4 and the RvSystem-Monitor contributors" |

| Upstream file | Adapted into | Changes |
|---|---|---|
| `rust/src/kernel/cpu.rs` | `crates/ztune-core/src/cpu.rs`, collectors in `crates/ztune/src/collect.rs` | Pure functions over text; `present` returns core ids and handles `0-3,6`; failed reads are `None`, never `0`/`"N/A"`; explicit two-sample load instead of global state; first sample reports no load instead of `-1` |
| `rust/src/kernel/thermal.rs` | `crates/ztune-core/src/thermal.rs`, `collect.rs` | Pure zone selection; the scale heuristic reports its divisor; implausible or unreadable zones give no value instead of `0.0`; per-core zone mapping not adapted |
| `rust/src/mm/memory.rs` | `crates/ztune-core/src/memory.rs`, `collect.rs` | Bytes instead of rounded decimal GB; missing keys are `None`; swap reported as swap, not "zram" |

Not adapted: the JNI bridge (`lib.rs`, `macros.rs`), the Vulkan probe, and the Kotlin app.

## Lynx

| Field | Value |
|---|---|
| URL | <https://github.com/Zy0x/Lynx> |
| Commit | `e56d6cd5005a543fa9798aa19b3627033593fbb0` (2026-10-22) |
| Licence | GPL-3.0 (`LICENSE`) |
| Copyright | No per-file notices upstream. Every adapted file was authored by Zy0x (Noir, `zy0x.noir@gmail.com`) per `git log --follow`; `Cargo.toml` names "ɴᴏɪʀ". Recorded as "Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors" |

| Upstream file | Adapted into | Changes |
|---|---|---|
| `native/lynxd/src/hardware/detector.rs` | `crates/ztune-core/src/platform.rs`, `collect.rs` | Pure scoring over observed signals; tie or low score is `unknown`, not "Generic" |
| `native/lynxd/src/hardware/capability.rs` (`resolve_storage`, `resolve_gpu`) | `crates/ztune-core/src/blockdev.rs`, `gpu.rs`, `collect.rs` | No built-in governor list when none is advertised; GPU node must be unambiguous; storage targets are non-virtual, non-removable |
| `native/lynxd/src/subsystems/io.rs` (`set_scheduler`) | `blockdev.rs`, `crates/ztune/src/apply.rs` | Value must be advertised by every target, else the request is rejected before any write |
| `native/lynxd/src/subsystems/gpu.rs` (`set_gov`) | `gpu.rs`, `apply.rs` | Governor must be advertised; `userspace` refused; no chmod fallback |
| `native/lynxd/src/sysfs/writer.rs` | `crates/ztune/src/writer.rs` | `ForcePermission` (chmod 0666) removed; caller-supplied read-back normalization |
| `native/lynxd/src/apply/executor.rs` | `crates/ztune/src/apply.rs` | One typed operation; boot-scoped journal written before the first write; rollback is read back; restore never re-forces an external change |

Not adapted, and why:

- Thermal bypass, "Extreme" profile, `lynx_thermal` KPM, vendor `thermal-engine.conf` and
  MediaTek `.tp` overrides: Zairenkai never writes thermal nodes or vendor configuration.
- CPU/GPU frequency locking, undervolt/overclock KPMs (`native/kpm/*`), `kpatch_bin`: writes
  outside the validated set; kernel patching is out of scope.
- Charging control (`hardware/charging.rs`), network, display, media, cpuset, IRQ, memory/VM
  writes: no validation record exists for any of them.
- Prebuilt binaries (`system/bin/lynxd`, `sqlite3`, `native/kpatch_bin`, the archived busybox
  applets): not source, not redistributed.
- `references/` (kernel DTS extracts, vendor thermal and power-HAL files) and
  `archive/device_reference/` (device dumps): third-party material with its own licences;
  nothing from them is used.
- `credit.md` thanks 18 people and teams. None of the adapted Rust files carries code
  attributed to them, and their history shows only Zy0x as author; the shell-era code they
  likely relate to (`archive/Chimera`) is not used.

## KonaBess

| Field | Value |
|---|---|
| URL | <https://github.com/libxzr/KonaBess> |
| Commit | `ff387dc7f630d7460c7a6561f83f71846217ee27` (2025-10-03) |
| Licence | GPL-3.0 (`LICENSE`) |
| Copyright | No per-file notices upstream. Adapted files by `git log --follow`: `GpuTableEditor.java`, `DtsHelper.java` LibXZR; `ChipInfo.java` LibXZR and Luffitys; the chip checks in `KonaBessCore.java` LibXZR, Jim Wu, Adithya R. Recorded as "Copyright (C) 2021-2025 LibXZR, Luffitys and the KonaBess contributors" |

| Upstream file | Adapted into | Changes |
|---|---|---|
| `ChipInfo.java`, `KonaBessCore.java` (`checkDevice`, `checkChip`, `checkSingleBin`) | `crates/ztune-core/src/gpu_table/chip.rs` | Port over text, no shell; "Lito v2" checked before "Lito"; OP4A79 property shortcut not ported; canoe level names (marked "todo" upstream) not used |
| `utils/DtsHelper.java` | `gpu_table/dts.rs` | `Option` instead of exceptions; single-cell values only |
| `GpuTableEditor.java` (`decode*`, `genTable`, `genBack`, `offset_*`) | `gpu_table/mod.rs` | The only edit is removing levels above a cap; all index properties shifted by the removed count and clamped; `throttle-pwrlevel` shifted, not forced to 0; only an exact `reg` property dropped |

Not adapted, and why: boot image dump/flash (`dd` to `/dev/block/bootdevice/by-name/*`),
`magiskboot` and `dtc` assets (prebuilt binaries under their own licences), adding levels,
frequency and voltage editing (`GpuVoltEditor.java`), table import/export (`TableIO.java`), the
UI. See `GPU_TABLE.md`.

## Not used

`Rve27/RvKernel-Manager` was cloned to look for Lynx under that name; nothing from it is used.

## SoC registry data

`crates/ztune-core/data/soc-registry.json` lists, with pinned commits, every source behind a
family rule or label (Linux mainline 7be4bc52 for socinfo machine names and SoC compatibles;
KonaBess ff387dc for device tree model codenames and their labels; LineageOS device trees for
the `gs101`, `gs201`, `zuma` and `parrot` board platforms). Identifiers are recorded as facts;
no kernel or device-tree code is copied. See `adr/0002-soc-family-registry.md`.

Rejected: Lynx `references/dts/**/*.dtsi` at e56d6cd are short reconstructions, not kernel
device trees, and are not used as evidence.
