// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! SoC vendor detection from kernel ground truth (device tree, driver nodes).
//!
//! Adapted from Lynx `native/lynxd/src/hardware/detector.rs` at commit
//! e56d6cd5005a543fa9798aa19b3627033593fbb0 (https://github.com/Zy0x/Lynx, GPL-3.0).
//! Changes (2026-10-22): scoring is a pure function over observed signals (the caller does
//! the I/O through a fixture-capable root); a tie or a score below the threshold reports
//! `unknown` instead of "Generic Linux", so nothing is tuned on a guessed vendor; signals
//! are reported as data. Signal paths, weights (+3/+2/+1) and the >=3, strictly-greater
//! decision rule are upstream's. `getprop` is deliberately not consulted, as upstream.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    Qualcomm,
    Mediatek,
    Unknown,
}

/// Nodes whose existence is a vendor signal: (path, vendor, weight).
pub const NODE_SIGNALS: &[(&str, Vendor, u32)] = &[
    ("/dev/ged", Vendor::Mediatek, 3),
    ("/dev/kgsl-3d0", Vendor::Qualcomm, 3),
    ("/proc/ppm", Vendor::Mediatek, 2),
    ("/proc/gpufreq", Vendor::Mediatek, 2),
    ("/proc/perfmgr", Vendor::Mediatek, 1),
    ("/sys/class/kgsl", Vendor::Qualcomm, 2),
    ("/sys/devices/soc0", Vendor::Qualcomm, 2),
    ("/sys/module/cpu_boost", Vendor::Qualcomm, 1),
];

/// Device tree `compatible` locations, tried in order.
pub const DT_COMPATIBLE: &[&str] = &[
    "/sys/firmware/devicetree/base/compatible",
    "/proc/device-tree/compatible",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    pub vendor: Vendor,
    pub qualcomm_score: u32,
    pub mediatek_score: u32,
    pub signals: Vec<String>,
}

/// `compatible` is NUL-separated; `present` lists which [`NODE_SIGNALS`] paths exist.
pub fn detect(compatible: Option<&[u8]>, present: &[&str]) -> Detection {
    let mut d = Detection {
        vendor: Vendor::Unknown,
        qualcomm_score: 0,
        mediatek_score: 0,
        signals: Vec::new(),
    };
    if let Some(raw) = compatible {
        let dt = String::from_utf8_lossy(raw)
            .to_ascii_lowercase()
            .replace('\0', " ");
        if dt.contains("mediatek") {
            d.mediatek_score += 3;
            d.signals
                .push("devicetree compatible: mediatek (+3)".into());
        }
        if dt.contains("qcom") || dt.contains("qualcomm") {
            d.qualcomm_score += 3;
            d.signals.push("devicetree compatible: qcom (+3)".into());
        }
    }
    for (path, vendor, weight) in NODE_SIGNALS {
        if !present.contains(path) {
            continue;
        }
        match vendor {
            Vendor::Mediatek => d.mediatek_score += weight,
            Vendor::Qualcomm => d.qualcomm_score += weight,
            Vendor::Unknown => {}
        }
        d.signals.push(format!("{path} exists (+{weight})"));
    }
    d.vendor = if d.mediatek_score > d.qualcomm_score && d.mediatek_score >= 3 {
        Vendor::Mediatek
    } else if d.qualcomm_score > d.mediatek_score && d.qualcomm_score >= 3 {
        Vendor::Qualcomm
    } else {
        Vendor::Unknown
    };
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualcomm_from_dt_and_nodes() {
        let d = detect(
            Some(b"qcom,kona-mtp\0qcom,kona\0"),
            &["/dev/kgsl-3d0", "/sys/class/kgsl"],
        );
        assert_eq!(d.vendor, Vendor::Qualcomm);
        assert_eq!(d.qualcomm_score, 8);
        assert_eq!(d.signals.len(), 3);
    }

    #[test]
    fn mediatek_and_ties() {
        assert_eq!(
            detect(Some(b"mediatek,mt6877\0"), &["/dev/ged"]).vendor,
            Vendor::Mediatek
        );
        // A spoofed or mixed device is not guessed.
        assert_eq!(
            detect(Some(b"qcom,x\0"), &["/dev/ged"]).vendor,
            Vendor::Unknown
        );
        assert_eq!(detect(None, &["/proc/perfmgr"]).vendor, Vendor::Unknown);
    }
}
