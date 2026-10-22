#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright 2026 ZairenkaiPrjctX contributors
"""Regenerate the `known_platforms` section of crates/ztune-core/data/soc-registry.json.

Reads SoC identifiers (facts, not code) from a pinned checkout of the Linux kernel:
- drivers/soc/qcom/socinfo.c: the names the Qualcomm socinfo driver reports in
  /sys/devices/soc0/machine;
- Documentation/devicetree/bindings/arm/{qcom,mediatek}.yaml,
  samsung/samsung-boards.yaml and google.yaml: SoC-level device tree compatibles.

Only `known_platforms` and its source entry are rewritten; family rules and annotations are
maintained by hand, each with its own source. A platform that is not listed is still detected
by the family rules; the list only says which identifiers mainline Linux knows.

Usage: update-soc-registry.py <linux checkout> [registry.json]
"""
import json
import re
import subprocess
import sys
from pathlib import Path

REGISTRY = Path(__file__).resolve().parent.parent / "crates/ztune-core/data/soc-registry.json"


def main(argv):
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    linux = Path(argv[1])
    out = Path(argv[2]) if len(argv) > 2 else REGISTRY
    commit = subprocess.run(
        ["git", "-C", str(linux), "rev-parse", "HEAD"], check=True, capture_output=True, text=True
    ).stdout.strip()
    socinfo = (linux / "drivers/soc/qcom/socinfo.c").read_text()
    machines = set()
    for m in re.finditer(r'qcom_board_id(?:_named)?\(([A-Z0-9_]+)(?:,\s*"([^"]+)")?\)', socinfo):
        machines.add((m.group(2) or m.group(1)).lower())
    bindings = linux / "Documentation/devicetree/bindings/arm"

    def compatibles(path, pattern):
        return sorted(set(re.findall(pattern, (bindings / path).read_text())))

    known = {
        "qualcomm": {
            "soc0_machine": sorted(machines),
            "dt_compatible": compatibles(
                "qcom.yaml", r"qcom,((?:msm|sdm|sm|sc|qcs|qcm|sa|sar|sdx|apq|x1e|x1p|qdu|qru)[0-9][0-9a-z]*)\b"
            ),
        },
        "mediatek": {"dt_compatible": compatibles("mediatek.yaml", r"mediatek,(mt[0-9]{4}[a-z]?)\b")},
        "samsung_exynos": {
            "dt_compatible": compatibles(
                "samsung/samsung-boards.yaml", r"samsung,(exynos(?:auto)?[0-9][0-9a-z]*)\b"
            )
        },
        "google_tensor": {"dt_compatible": compatibles("google.yaml", r"google,(gs[0-9]{3})\b")},
    }
    reg = json.loads(out.read_text()) if out.exists() else {"schema_version": 1, "sources": []}
    reg["sources"] = [s for s in reg.get("sources", []) if s["id"] != "linux-mainline"] + [
        {
            "id": "linux-mainline",
            "url": "https://github.com/torvalds/linux",
            "commit": commit,
            "paths": [
                "drivers/soc/qcom/socinfo.c",
                "Documentation/devicetree/bindings/arm/qcom.yaml",
                "Documentation/devicetree/bindings/arm/mediatek.yaml",
                "Documentation/devicetree/bindings/arm/samsung/samsung-boards.yaml",
                "Documentation/devicetree/bindings/arm/google.yaml",
            ],
            "license": "GPL-2.0 (kernel); identifiers are used as facts, no code is copied",
            "use": "known_platforms",
        }
    ]
    reg["known_platforms"] = known
    out.write_text(json.dumps(reg, indent=2, sort_keys=False) + "\n")
    n = sum(len(v) for f in known.values() for v in f.values())
    print(f"{out}: {n} identifiers from linux {commit}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
