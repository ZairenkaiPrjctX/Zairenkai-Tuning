// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2021-2025 LibXZR, Luffitys and the KonaBess contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Snapdragon chip identification from a decompiled device tree, and the per-chip facts
//! the GPU table editor needs.
//!
//! Adapted from KonaBess `app/src/main/java/xzr/konabess/ChipInfo.java` and the chip checks
//! in `KonaBessCore.java` (`checkDevice`, `checkChip`, `checkSingleBin`) at commit
//! ff387dc7f630d7460c7a6561f83f71846217ee27 (https://github.com/libxzr/KonaBess, GPL-3.0).
//! Changes (2026-10-22): Rust port over DTS text (upstream ran `cat | grep` in a shell);
//! "Lito v2" is checked before "Lito" (upstream's order made lito_v2 unreachable); the
//! OnePlus "OP4A79" property shortcut is not ported (it reads a system property, not the
//! DTS); RPMh level names are kept only for decoding, and chips whose upstream names are
//! marked "todo" (canoe) report raw values; the Android string resources are not ported.
//! Chip patterns, single-bin rules and maximum level counts are upstream's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chip {
    Kona,
    KonaSingleBin,
    Msmnile,
    MsmnileSingleBin,
    Lahaina,
    LahainaSingleBin,
    Lito,
    Lagoon,
    Shima,
    Yupik,
    WaipioSingleBin,
    CapeSingleBin,
    Kalama,
    Diwali,
    UkeeSingleBin,
    Pineapple,
    CliffsSingleBin,
    Cliffs7SingleBin,
    KalamaSgSingleBin,
    Sun,
    Canoe,
}

/// `model` substrings, in the order upstream checks them. Kona, msmnile and lahaina pick
/// their single-bin variant from the table layout.
const PATTERNS: &[(&[&str], Family)] = &[
    (&["kona v2.1"], Family::Kona),
    (&["SM8150 v2"], Family::Msmnile),
    (&["Lahaina V2.1", "Lahaina v2.1"], Family::Lahaina),
    (&["Lito v2", "Lito"], Family::Fixed(Chip::Lito)),
    (&["Lagoon"], Family::Fixed(Chip::Lagoon)),
    (&["Shima"], Family::Fixed(Chip::Shima)),
    (&["Yupik"], Family::Fixed(Chip::Yupik)),
    (
        &["Waipio", "Waipio v2"],
        Family::Fixed(Chip::WaipioSingleBin),
    ),
    (&["Cape"], Family::Fixed(Chip::CapeSingleBin)),
    (&["Kalama v2", "KalamaP v2"], Family::Fixed(Chip::Kalama)),
    (&["Diwali"], Family::Fixed(Chip::Diwali)),
    (&["Ukee"], Family::Fixed(Chip::UkeeSingleBin)),
    (
        &["Pineapple v2", "PineappleP v2"],
        Family::Fixed(Chip::Pineapple),
    ),
    (&["Cliffs SoC"], Family::Fixed(Chip::CliffsSingleBin)),
    (&["Cliffs 7 SoC"], Family::Fixed(Chip::Cliffs7SingleBin)),
    (&["KalamaP SG SoC"], Family::Fixed(Chip::KalamaSgSingleBin)),
    (
        &[
            "Sun v2 SoC",
            "Sun Alt. Thermal Profile v2 SoC",
            "SunP v2 SoC",
            "SunP v2 Alt. Thermal Profile SoC",
        ],
        Family::Fixed(Chip::Sun),
    ),
    (
        &["Canoe v2 SoC", "CanoeP v2 SoC"],
        Family::Fixed(Chip::Canoe),
    ),
];

#[derive(Debug, Clone, Copy)]
enum Family {
    Kona,
    Msmnile,
    Lahaina,
    Fixed(Chip),
}

/// The `model = "...";` value of the root node, if any line declares one.
pub fn model(dts: &str) -> Option<String> {
    dts.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("model")?.trim_start().strip_prefix('=')?;
        let v = rest.trim().strip_suffix(';')?.trim();
        Some(v.strip_prefix('"')?.strip_suffix('"')?.to_string())
    })
}

/// Upstream's single-bin test: a `qcom,gpu-pwrlevels {` node (no bin suffix).
pub fn has_single_bin_table(dts: &str) -> bool {
    dts.lines().any(|l| l.trim() == "qcom,gpu-pwrlevels {")
}

/// Identifies the chip from the lines that mention `model`, as upstream greps them.
pub fn identify(dts: &str) -> Option<Chip> {
    let model_lines: Vec<&str> = dts.lines().filter(|l| l.contains("model")).collect();
    let hit = |p: &str| model_lines.iter().any(|l| l.contains(p));
    let single = has_single_bin_table(dts);
    for (patterns, family) in PATTERNS {
        if patterns.iter().any(|p| hit(p)) {
            return Some(match family {
                Family::Kona if single => Chip::KonaSingleBin,
                Family::Kona => Chip::Kona,
                Family::Msmnile if single => Chip::MsmnileSingleBin,
                Family::Msmnile => Chip::Msmnile,
                Family::Lahaina if single => Chip::LahainaSingleBin,
                Family::Lahaina => Chip::Lahaina,
                Family::Fixed(c) => *c,
            });
        }
    }
    None
}

impl Chip {
    /// Single-bin chips keep one `qcom,gpu-pwrlevels` table; the others one per speed bin.
    pub fn single_bin(self) -> bool {
        matches!(
            self,
            Chip::KonaSingleBin
                | Chip::MsmnileSingleBin
                | Chip::LahainaSingleBin
                | Chip::WaipioSingleBin
                | Chip::CapeSingleBin
                | Chip::UkeeSingleBin
                | Chip::CliffsSingleBin
                | Chip::Cliffs7SingleBin
                | Chip::KalamaSgSingleBin
        )
    }

    pub fn max_table_levels(self) -> usize {
        match self {
            Chip::CapeSingleBin
            | Chip::WaipioSingleBin
            | Chip::Kalama
            | Chip::Diwali
            | Chip::UkeeSingleBin
            | Chip::Pineapple
            | Chip::CliffsSingleBin
            | Chip::Cliffs7SingleBin
            | Chip::KalamaSgSingleBin
            | Chip::Sun
            | Chip::Canoe => 16,
            _ => 11,
        }
    }

    /// RPMh corner votes and names, for decoding `qcom,level` / `qcom,cx-level`.
    pub fn rpmh_levels(self) -> Option<(&'static [u32], &'static [&'static str], u32)> {
        Some(match self {
            Chip::Msmnile | Chip::MsmnileSingleBin => (BASE17, BASE17_NAMES, 1),
            Chip::Lahaina => (LAHAINA, LAHAINA_NAMES, 0),
            Chip::Kalama
            | Chip::Pineapple
            | Chip::CliffsSingleBin
            | Chip::Cliffs7SingleBin
            | Chip::KalamaSgSingleBin => (KALAMA, KALAMA_NAMES, 0),
            Chip::Sun => (SUN, SUN_NAMES, 0),
            Chip::Canoe => return None,
            _ => (BASE17, BASE17_NAMES, 0),
        })
    }

    /// Name of an RPMh vote, if the chip's table has it.
    pub fn level_name(self, vote: u64) -> Option<&'static str> {
        let (levels, names, offset) = self.rpmh_levels()?;
        levels
            .iter()
            .position(|l| u64::from(l + offset) == vote)
            .map(|i| names[i])
    }
}

const BASE17: &[u32] = &[
    16, 48, 56, 64, 80, 96, 128, 144, 192, 224, 256, 320, 336, 352, 384, 400, 416,
];
const BASE17_NAMES: &[&str] = &[
    "RETENTION",
    "MIN_SVS",
    "LOW_SVS_D1",
    "LOW_SVS",
    "LOW_SVS_L1",
    "LOW_SVS_L2",
    "SVS",
    "SVS_L0",
    "SVS_L1",
    "SVS_L2",
    "NOM",
    "NOM_L1",
    "NOM_L2",
    "NOM_L3",
    "TURBO",
    "TURBO_L0",
    "TURBO_L1",
];
const LAHAINA: &[u32] = &[
    16, 48, 56, 64, 80, 96, 128, 144, 192, 224, 256, 320, 336, 352, 384, 400, 416, 432, 448, 464,
];
const LAHAINA_NAMES: &[&str] = &[
    "RETENTION",
    "MIN_SVS",
    "LOW_SVS_D1",
    "LOW_SVS",
    "LOW_SVS_L1",
    "LOW_SVS_L2",
    "SVS",
    "SVS_L0",
    "SVS_L1",
    "SVS_L2",
    "NOM",
    "NOM_L1",
    "NOM_L2",
    "NOM_L3",
    "TURBO",
    "TURBO_L0",
    "TURBO_L1",
    "TURBO_L2",
    "SUPER_TURBO",
    "SUPER_TURBO_NO_CPR",
];
const KALAMA: &[u32] = &[
    16, 48, 52, 56, 60, 64, 72, 80, 96, 128, 144, 192, 224, 256, 288, 320, 336, 384, 400, 416, 432,
    448, 464, 480,
];
const KALAMA_NAMES: &[&str] = &[
    "RETENTION",
    "MIN_SVS",
    "LOW_SVS_D2",
    "LOW_SVS_D1",
    "LOW_SVS_D0",
    "LOW_SVS",
    "LOW_SVS_P1",
    "LOW_SVS_L1",
    "LOW_SVS_L2",
    "SVS",
    "SVS_L0",
    "SVS_L1",
    "SVS_L2",
    "NOM",
    "NOM_L0",
    "NOM_L1",
    "NOM_L2",
    "TURBO",
    "TURBO_L0",
    "TURBO_L1",
    "TURBO_L2",
    "TURBO_L3",
    "SUPER_TURBO",
    "SUPER_TURBO_NO_CPR",
];
const SUN: &[u32] = &[
    16, 48, 50, 52, 56, 60, 64, 72, 80, 96, 128, 144, 192, 224, 256, 288, 320, 336, 384, 400, 416,
    432, 448, 452, 464, 480,
];
const SUN_NAMES: &[&str] = &[
    "RETENTION",
    "MIN_SVS",
    "LOW_SVS_D3",
    "LOW_SVS_D2",
    "LOW_SVS_D1",
    "LOW_SVS_D0",
    "LOW_SVS",
    "LOW_SVS_P1",
    "LOW_SVS_L1",
    "LOW_SVS_L2",
    "SVS",
    "SVS_L0",
    "SVS_L1",
    "SVS_L2",
    "NOM",
    "NOM_L0",
    "NOM_L1",
    "NOM_L2",
    "TURBO",
    "TURBO_L0",
    "TURBO_L1",
    "TURBO_L2",
    "TURBO_L3",
    "TURBO_L4",
    "SUPER_TURBO",
    "SUPER_TURBO_NO_CPR",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_line_up() {
        for (l, n) in [
            (BASE17, BASE17_NAMES),
            (LAHAINA, LAHAINA_NAMES),
            (KALAMA, KALAMA_NAMES),
            (SUN, SUN_NAMES),
        ] {
            assert_eq!(l.len(), n.len());
        }
    }

    #[test]
    fn identify_chips() {
        let kona = "/ {\n\tmodel = \"Qualcomm Technologies, Inc. kona v2.1 SoC\";\n\
                    \tqcom,gpu-pwrlevels-0 {\n";
        assert_eq!(identify(kona), Some(Chip::Kona));
        assert_eq!(
            model(kona).as_deref(),
            Some("Qualcomm Technologies, Inc. kona v2.1 SoC")
        );
        let single =
            "model = \"Qualcomm Technologies, Inc. kona v2.1 SoC\";\nqcom,gpu-pwrlevels {\n";
        assert_eq!(identify(single), Some(Chip::KonaSingleBin));
        assert_eq!(identify("model = \"Lito v2\";"), Some(Chip::Lito));
        assert_eq!(
            identify("model = \"Cliffs 7 SoC\";"),
            Some(Chip::Cliffs7SingleBin)
        );
        assert_eq!(identify("model = \"Exynos 2100\";"), None);
        assert_eq!(identify("compatible = \"qcom,kona\";"), None);
    }

    #[test]
    fn level_names() {
        assert_eq!(Chip::Kona.level_name(256), Some("NOM"));
        assert_eq!(Chip::Msmnile.level_name(257), Some("NOM"));
        assert_eq!(Chip::Msmnile.level_name(256), None);
        assert_eq!(Chip::Canoe.level_name(256), None);
        assert_eq!(Chip::Kalama.max_table_levels(), 16);
        assert!(!Chip::Kona.single_bin() && Chip::KonaSingleBin.single_bin());
    }
}
