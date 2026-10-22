// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Rve27 (Radika), Pavelc4 and the RvSystem-Monitor contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Thermal zone selection for CPU and GPU temperature.
//!
//! Adapted from RvSystem-Monitor `rust/src/kernel/thermal.rs` at commit
//! b07fe27598555e49084bbcc40be558803286b9ef (https://github.com/Rve27/RvSystem-Monitor,
//! GPL-3.0). Changes (2026-10-22): pure functions over zone lists instead of cached file
//! descriptors; the scale heuristic reports which divisor it used, so the contract can say
//! the value is derived; an unreadable or implausible zone yields no value instead of 0.0;
//! per-core zone mapping was not adapted. The zone priority lists, the 5-120 degree
//! plausibility window and the exact-then-substring matching order are upstream's.
//!
//! This module only reads temperatures. Zairenkai never writes thermal nodes.

/// Zone types tried for the CPU temperature, most specific first (upstream order).
pub const CPU_ZONES: &[&str] = &[
    "soc_max",
    "soc_thermal",
    "soc-thermal",
    "cpu_max",
    "cpu-thermal",
    "msm_therm",
    "mtktsap",
    "ap_ntc",
    "apc",
    "cpuss",
    "cpu",
    "soc",
    "tsens_tz_sensor0",
    "thermal-cpufreq",
];

/// Zone types tried for the GPU temperature (upstream order).
pub const GPU_ZONES: &[&str] = &[
    "gpu-thermal",
    "gpu0-thermal",
    "gpuss-0-usr",
    "gpu",
    "tsens_tz_sensor9",
];

/// Lowest and highest temperature accepted as a real sensor reading, in degrees Celsius.
pub const PLAUSIBLE: (f64, f64) = (5.0, 120.0);

/// A raw `temp` value converted to degrees Celsius, with the divisor that was applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Celsius {
    pub value: f64,
    pub divisor: u32,
}

/// Converts a raw `temp` attribute. The ABI is millidegrees; some vendor zones report
/// decidegrees or degrees, which upstream detects by magnitude.
pub fn normalize(raw: &str) -> Option<Celsius> {
    let t: f64 = raw.trim().parse().ok()?;
    if !t.is_finite() {
        return None;
    }
    let divisor = if t > 1000.0 {
        1000
    } else if t > 150.0 {
        10
    } else {
        1
    };
    let value = (t / f64::from(divisor) * 10.0).round() / 10.0;
    Some(Celsius { value, divisor })
}

pub fn plausible(c: &Celsius) -> bool {
    c.value > PLAUSIBLE.0 && c.value < PLAUSIBLE.1
}

/// One thermal zone as read from `/sys/class/thermal/thermal_zoneN`.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub dir: String,
    /// `type`, lower-cased and trimmed.
    pub kind: String,
    pub temp: Option<Celsius>,
}

/// Picks the zone for a priority list: an exact type match first, then a substring match,
/// each only if its reading is plausible. Returns the index into `zones`.
pub fn select(zones: &[Zone], priority: &[&str]) -> Option<usize> {
    let ok = |z: &Zone| z.temp.as_ref().is_some_and(plausible);
    for want in priority {
        if let Some(i) = zones.iter().position(|z| z.kind == *want && ok(z)) {
            return Some(i);
        }
    }
    for want in priority {
        // Deterministic among several matches: zones are sorted by directory name.
        if let Some(i) = zones.iter().position(|z| z.kind.contains(want) && ok(z)) {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(dir: &str, kind: &str, raw: &str) -> Zone {
        Zone {
            dir: dir.into(),
            kind: kind.into(),
            temp: normalize(raw),
        }
    }

    #[test]
    fn scales() {
        assert_eq!(
            normalize("42500").unwrap(),
            Celsius {
                value: 42.5,
                divisor: 1000
            }
        );
        assert_eq!(
            normalize("425").unwrap(),
            Celsius {
                value: 42.5,
                divisor: 10
            }
        );
        assert_eq!(
            normalize("42").unwrap(),
            Celsius {
                value: 42.0,
                divisor: 1
            }
        );
        assert!(normalize("").is_none());
        assert!(normalize("abc").is_none());
    }

    #[test]
    fn exact_before_substring_and_implausible_skipped() {
        let zones = vec![
            z("thermal_zone0", "cpu-0-0-usr", "45000"),
            z("thermal_zone1", "soc", "-273000"),
            z("thermal_zone2", "cpuss-0", "41000"),
            z("thermal_zone3", "cpu", "39000"),
        ];
        // "soc" is exact but implausible; "cpu" exact wins over substring matches.
        assert_eq!(select(&zones, CPU_ZONES), Some(3));
        let zones = vec![z("thermal_zone0", "gpuss-1-usr", "50000")];
        assert_eq!(select(&zones, GPU_ZONES), Some(0));
        assert_eq!(select(&[], GPU_ZONES), None);
    }
}
