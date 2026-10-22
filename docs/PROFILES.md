# Device profiles and support tiers

`ztune device` prints the exact-device identity, the SoC family identification and the
resolved support tier. Profiles decide whether a write control may be used; nothing else does.

## Identity keys

Each is a separate compatibility key; a marketing name is never one:
`codename` (`ro.product.vendor.device`), `board_platform` (`ro.board.platform`), SoC family and
`platform_id` (device tree compatible, `/sys/devices/soc0/{family,machine}`), `android_api`
(`ro.build.version.sdk`), `vendor_build_id` (`ro.vendor.build.id`), `kernel_release`
(`/proc/sys/kernel/osrelease`), ABIs, cpufreq policy topology. The GPU and driver come from
Zairenkai-Graphics-Platform; ztune does not guess them. Fingerprints and serials are not read.

## Tiers

| Tier | When | Writes |
|---|---|---|
| D | A `device_validated`, hardware-tested profile matches every key exactly | Only the controls and values it grants, through Core |
| S | SoC family identified | None; read-only diagnostics, unverified recommendations |
| G | No family, generic cpufreq/devfreq/block nodes readable | None |
| X | Unknown and nothing readable, or conflicting signals | None |
| U | App-level only (shared vocabulary; not produced by ztune) | None |

## Profile schema (version 1)

Strict JSON, unknown fields rejected (`ztune_core::profile::Profile`): `profile_id`,
`profile_version`, `status` (`candidate`, `source_verified`, `fixture_tested`,
`device_validated`, `deprecated`, `rejected`, `unsupported`), `synthetic`, `identity` (the keys
above, with exact vendor builds and kernels), `controls` (typed control ids with allowed
values, never paths or commands), `provenance` (URL, full commit, paths, licence) and
`validation` (`hardware_tested`, evidence records, `reviewed_at`). A `device_validated`
profile without hardware evidence, provenance or exact builds is rejected.

Profiles ship inside the binary (`profiles/index.json`); the set is empty until a device has a
validation record. `--profiles-dir` adds profiles for tests and maintainers; a profile marked
`synthetic` is honoured only against a fixture root.
