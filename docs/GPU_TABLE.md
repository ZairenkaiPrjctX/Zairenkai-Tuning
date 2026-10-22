# GPU frequency table (adapted from KonaBess)

KonaBess changes a Snapdragon GPU's frequency and voltage table by dumping the boot or
vendor_boot partition, decompiling its device tree, editing the `qcom,gpu-pwrlevels` nodes,
recompiling, repacking and flashing it back with `dd`. A mistake there can stop the device
from booting.

Zairenkai adapts only the parts that can be checked on a computer and cannot raise anything
above the stock table:

| Step | Zairenkai | Why |
|---|---|---|
| Identify the chip from the DTS `model` | `ztune gpu-table inspect` | KonaBess's chip patterns and single-bin rule |
| Parse bins, levels, RPMh votes | `inspect` | KonaBess's table parser |
| Edit | `plan` / `stage`: remove levels above a frequency cap | Underclock only: no new level, no frequency or voltage edit, so no level exceeds a stock one |
| Fix level indices | Shift `initial`, `ca-target`, `throttle` power levels by the number removed, clamped at 0 | Keeps each index pointing at the same level; never forces `throttle-pwrlevel` to 0 |
| Dump, unpack, decompile, recompile, repack, flash | **Not implemented** | Needs `magiskboot` and `dtc` binaries, the right partition and slot, and a hardware validation record; none exists |

## Gates

1. **Opt-in.** Nothing in the module runs `gpu-table`: not at install, not at boot, not from
   the WebUI. A person runs it by hand.
2. **Device match.** `plan` and `stage` require the DTS `model` to equal the running device's
   `/sys/firmware/devicetree/base/model`. If the device model cannot be read, the request is
   refused (`device_mismatch`).
3. **Input validation.** At most 16 MiB of UTF-8 device tree source mentioning `qcom,`; the
   chip must be recognised; the bin must exist; levels must be ordered from the highest
   frequency down; at least one non-zero level must remain; a cap that removes nothing is
   refused.
4. **Backup.** `stage` writes a byte-identical copy of the input (`original.dts`) next to the
   edited tree (`modified.dts`) and `plan.json`, into a new directory (mode 0700) that must not
   exist. Both files are read back; the edited one is re-parsed and its levels compared with
   the plan.
5. **Explicit confirmation.** `stage` needs `--confirm <token>`; the token is printed by `plan`
   and binds the input's bytes, the bin, the cap and the device model.

```sh
ztune gpu-table inspect --dts 0.dts
ztune gpu-table plan  --dts 0.dts --bin 0 --max-freq-hz 600000000
ztune gpu-table stage --dts 0.dts --bin 0 --max-freq-hz 600000000 \
      --confirm <token from plan> --out /data/local/tmp/gpu-cap
```

The staged `modified.dts` is a file; using it on a device (recompiling and flashing) is a
manual, unsupported step outside Zairenkai.

## Status

Tested only against synthetic device trees (`crates/ztune/tests/fixtures/kona-synthetic.dts`
and the trees in `crates/ztune-core/src/gpu_table/mod.rs`). No real DTB has been inspected,
and nothing has been flashed. Chip support is KonaBess's list, unverified here.
