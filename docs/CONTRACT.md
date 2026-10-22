# Integration contract: `zairenkai.tuning` schema 1

`ztune` is a command-line program. Zairenkai-Core runs it as a separate process (as root,
with a fixed argument vector, no shell) and reads one JSON document from its standard
output. Zairenkai-App never runs it while Core answers; it receives what Core reports. This
page is the whole interface: a consumer needs nothing from this repository but this text.

## Envelope

Every document is one JSON object:

| Field | Type | Meaning |
|---|---|---|
| `schema` | string | Always `"zairenkai.tuning"` |
| `schema_version` | integer | `1`. A consumer must refuse any other value |
| `kind` | string | `identity`, `capabilities`, `telemetry`, `operation`, `restore`, `gpu_table` or `error` |
| `component` | object | `{"name": "zairenkai-tuning", "version": "<semver>"}` |

Within schema 1, fields and enum values may be added, never removed or changed in meaning.
Consumers must ignore unknown fields.

## Exit codes

`0` success (including a dry run and "no change"), `1` failed (I/O, busy, verification or
rollback), `2` usage error, `4` refused (invalid value, safety rule, device mismatch, missing
confirmation), `5` unsupported on this device or not implemented. The document is printed in
every case; a consumer should rely on the document, and use the exit code as a summary. On a
non-zero exit, one line `ztune: <error.code>: <error.message>` is also written to stderr, for
callers that keep only stderr on failure.

## Commands

| Command | `kind` | Writes? |
|---|---|---|
| `ztune identity` | `identity` | no |
| `ztune capabilities` | `capabilities` | no |
| `ztune telemetry [--interval-ms N]` | `telemetry` | no |
| `ztune apply <operation> <value> [--dry-run]` | `operation` | yes, unless `--dry-run` |
| `ztune restore` | `restore` | yes (restores) |
| `ztune gpu-table inspect\|plan\|stage …` | `gpu_table` | `stage` writes files into a new directory; never a partition |

Global options: `--sysroot DIR` (tests), `--state-dir DIR` (default
`/data/adb/zairenkai/tuning`, holds the journal and lock).

## Readings

Every measured value is a reading:

```json
{"status": "ok", "value": 1804800, "unit": "kHz", "source": "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"}
{"status": "unsupported", "value": null, "unit": "celsius", "detail": "no zone matches …"}
```

`status`: `ok`, `unsupported` (node absent), `permission_denied`, `unavailable` (node gave no
value now), `parse_error`, `not_sampled` (needs two samples). A reading whose status is not
`ok` has `value: null`; a number is never substituted.

`unit`: `kHz`, `Hz`, `percent`, `celsius`, `bytes`, `text`, `count`. Values are not rescaled:
CPU frequency in kHz (cpufreq ABI), GPU frequency in Hz (devfreq ABI), memory in bytes.

## `identity`

`commit` (string or null), `license` (`"GPL-3.0-only"`), `source` (repository URL),
`contract` (`{schema, schema_version}`), `operations` (names), `upstreams` (name, url,
commit, license of each adapted project).

## `telemetry`

| Field | Content |
|---|---|
| `interval_ms` | Time between the two `/proc/stat` samples; `0` means one sample, so every load is `not_sampled` |
| `platform` | `vendor` (`qualcomm`, `mediatek`, `unknown`), `qualcomm_score`, `mediatek_score`, `signals` |
| `cpu.present` | Reading of core ids |
| `cpu.load` | Reading, percent, all cores |
| `cpu.cores[]` | `id`, `cur_freq`, `min_freq`, `max_freq` (kHz), `governor` (text), `load` (percent) |
| `thermal.cpu`, `thermal.gpu` | Reading, celsius. `source` names the zone; `detail` names the divisor applied to the raw value (1000, 10 or 1) |
| `memory` | `total`, `available`, `used`, `swap_total`, `swap_used` (bytes) |
| `gpu` | `devfreq` (directory or null), `cur_freq` (Hz), `governor` (text) |
| `storage[]` | `device`, `scheduler` (text, current) |

## `capabilities`

`capabilities[]`: `id`, `state`, optional `operation`, `allowed_values`, `current`, and
`evidence` (paths or reasons).

| `state` | Meaning |
|---|---|
| `available` | Read capability; read successfully now |
| `not_verified` | Write capability; nodes and advertised values exist, no write read back on this boot |
| `verified` | Write capability; a write was read back on this boot |
| `unavailable` | Exists but cannot be used now (permission, read error) |
| `unsupported` | Absent or ambiguous on this device |

Capability ids in schema 1: `telemetry.cpu`, `telemetry.thermal.cpu`,
`telemetry.thermal.gpu`, `telemetry.memory`, `telemetry.gpu`, `io.scheduler`,
`gpu.governor`. `allowed_values` lists only values the device advertises and Zairenkai
accepts; a consumer must not offer anything else.

## Operations

| Operation | Targets | Values |
|---|---|---|
| `io.scheduler` | `queue/scheduler` of every internal block device (non-virtual, not removable, with a choice) | A scheduler every target advertises |
| `gpu.governor` | `governor` of the GPU devfreq node (KGSL, else the single GPU-named devfreq node) | A governor the node advertises, except `userspace` |

Before anything is written, `ztune` validates the value (1-32 of `[a-z0-9_-]`, advertised by
every target, not refused), takes a lock, and saves each original value in a journal tagged
with the boot id. Each target is written and read back; on the first failure every target
already written is restored and read back.

An `operation` document:

| Field | Content |
|---|---|
| `request` | `op`, `value`, `dry_run`, `status`: `accepted` or `rejected`. Says nothing about the device |
| `operation.status` | `not_started` (rejected, or dry run), `no_change` (every target already had the value), `applied_verified`, `rolled_back` (failure, every written target restored and read back), `failed` (failure, a target could not be restored) |
| `operation.targets[]` | `target`, `path`, `before`, `requested`, `actual` (read back after the operation, or after rollback), `outcome` |
| `error` | Present on rejection or failure: `{code, message}` |

`outcome`: `unchanged`, `written`, `would_write`, `restored`, `restore_failed`,
`write_failed`, `verification_failed`, `not_attempted`, `changed_externally`.

`restore` documents have `status`, `targets[]` and optional `error` with the same
vocabulary. A node whose value is neither the original nor the value `ztune` wrote is
reported `changed_externally` and left alone. Kernel attributes reset at reboot; a journal
from an earlier boot is discarded, never replayed.

## Errors

`error.code`: `usage`, `invalid_value`, `unsupported`, `refused`, `device_mismatch`,
`confirmation_required`, `permission_denied`, `busy`, `io`, `verification_failed`,
`not_implemented`.

## `gpu_table`

See `GPU_TABLE.md`. `inspection`, `plan` or `staged` objects; `plan.flash` is always
`"not_implemented: …"`.
