# ADR 0001: Tuning writes are Tier D only; the executor question stays open

Status: accepted for this change (2026-10-22); one question is left to the maintainer.

## Context

Two maintainer instructions apply to this repository:

1. *GPL component brief* (2026-10-22, this repository's creation): adapt Lynx tuning
   functions that are "truly device compatible" into a GPL-3.0 component, with a versioned
   contract carrying operation status, actual results and structured errors.
2. *Ecosystem master prompt and cross-device architecture* (2026-10-22, later): Tuning owns
   profile resolution and explanation and is "never a root executor"; every system write needs
   an exact-device/build profile in state `device_validated` with physical-device evidence;
   a family match never authorizes a write; graphics controls stay read-only until validated.

The first version of `ztune` (commits ab09e36, 450a624) let `ztune apply` write
`io.scheduler` and `gpu.governor` whenever the device advertised the value, and Core reported
both controls read-write. That satisfies (1) but not (2).

## Decision

- `ztune apply` (dry runs included) is refused with `not_authorized` unless the support-tier
  resolver returns Tier D: an exact-device profile, `device_validated`, hardware-tested, with
  provenance, matching codename, SoC/platform id, board platform, Android API range, vendor
  build id, kernel release and primary ABI, and granting that control and value
  (`ztune_core::profile`). Profiles that require a GPU identity are never satisfied by ztune.
- No profile is shipped (`profiles/index.json` is empty), because no device has a validation
  record. **No write is possible on any device today.** Synthetic test profiles only count
  against a fixture root.
- Write capabilities carry `authorization` (`write_authorized`, `tier`, `profile_id`,
  `granted_values`, `reason`). Core reports them read-write only when `write_authorized` is true.
- The verified write path (journal before write, read-back, rollback, restore) is kept, so a
  future validated control does not need new, less-tested code.

## Open question for the maintainer

Whether a write executor belongs in Tuning at all. Options:

| Option | Effect |
|---|---|
| A. Keep the gated executor in `ztune` (this ADR) | Matches brief (1); writes stay impossible until a Tier D profile exists; Core still decides and re-reads |
| B. Remove `apply`/`restore` from `ztune`, keep it read-only + profiles | Matches "never a root executor" literally; a future validated control needs a new owner |
| C. A Core-owned executor | Core is Apache-2.0: it may not port ztune's Lynx-derived code; it would need an independent implementation |

Until decided, option A applies and fails closed.
