// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! The exact-device identity tuple a profile is matched against.
//!
//! Every field is a separate compatibility key (codename, SoC/platform, Android API, vendor
//! build, kernel, ABI, CPU topology). A marketing name is never one of them. The GPU and its
//! driver are identified by Zairenkai-Graphics-Platform from a real Vulkan/GLES context, not
//! here; a profile that requires a GPU match therefore cannot authorize a write through ztune
//! alone. Build fingerprints and serial numbers are not collected.

use serde::{Deserialize, Serialize};

use crate::soc;

/// build.prop files read for identity, in precedence order (first value wins).
pub const BUILD_PROPS: &[&str] = &[
    "/vendor/build.prop",
    "/odm/etc/build.prop",
    "/system/build.prop",
];

/// Parses `key=value` lines; comments and malformed lines are skipped.
pub fn parse_props(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let k = k.trim();
            (!k.is_empty() && !k.contains(char::is_whitespace))
                .then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// First non-empty value of any of `keys` across property sets in precedence order.
pub fn prop(sets: &[Vec<(String, String)>], keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        sets.iter().find_map(|set| {
            set.iter()
                .find(|(key, v)| key == k && !v.is_empty())
                .map(|(_, v)| v.clone())
        })
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuPolicy {
    /// The first CPU of the cpufreq policy.
    pub policy: u32,
    pub cpus: Vec<u32>,
    pub max_freq_khz: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceIdentity {
    pub codename: Option<String>,
    pub manufacturer: Option<String>,
    pub board_platform: Option<String>,
    pub android_api: Option<u32>,
    pub vendor_build_id: Option<String>,
    pub vendor_build_incremental: Option<String>,
    pub kernel_release: Option<String>,
    pub abis: Vec<String>,
    pub cpu_policies: Vec<CpuPolicy>,
    pub soc: soc::Identity,
}

impl DeviceIdentity {
    /// Builds the identity from property sets and the other observed facts.
    pub fn from_parts(
        props: &[Vec<(String, String)>],
        kernel_release: Option<String>,
        cpu_policies: Vec<CpuPolicy>,
        mut observed: soc::Observed,
    ) -> Self {
        let get = |keys: &[&str]| prop(props, keys);
        observed.board_platform = observed
            .board_platform
            .or_else(|| get(&["ro.board.platform"]));
        observed.soc_manufacturer = observed
            .soc_manufacturer
            .or_else(|| get(&["ro.soc.manufacturer"]));
        observed.soc_model = observed.soc_model.or_else(|| get(&["ro.soc.model"]));
        let abis = get(&["ro.vendor.product.cpu.abilist", "ro.product.cpu.abilist"])
            .map(|l| {
                l.split(',')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        DeviceIdentity {
            codename: get(&["ro.product.vendor.device", "ro.product.device"]),
            manufacturer: get(&["ro.product.vendor.manufacturer", "ro.product.manufacturer"]),
            board_platform: observed.board_platform.clone(),
            android_api: get(&["ro.build.version.sdk"]).and_then(|v| v.parse().ok()),
            vendor_build_id: get(&["ro.vendor.build.id"]),
            vendor_build_incremental: get(&["ro.vendor.build.version.incremental"]),
            kernel_release,
            abis,
            cpu_policies,
            soc: soc::identify(&observed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_precedence_and_identity() {
        let vendor = parse_props(
            "# comment\nro.product.vendor.device=garnet\nro.board.platform=parrot\n\
             ro.vendor.build.id=AP3A.240905.015\nro.vendor.product.cpu.abilist=arm64-v8a\nbroken line\n",
        );
        let system =
            parse_props("ro.product.device=other\nro.build.version.sdk=35\nro.board.platform=x\n");
        let id = DeviceIdentity::from_parts(
            &[vendor, system],
            Some("5.10.226-android12-9".into()),
            vec![],
            soc::Observed::default(),
        );
        assert_eq!(id.codename.as_deref(), Some("garnet"));
        assert_eq!(id.board_platform.as_deref(), Some("parrot"));
        assert_eq!(id.android_api, Some(35));
        assert_eq!(id.abis, vec!["arm64-v8a"]);
        assert_eq!(id.soc.family, soc::Family::Qualcomm);
        assert_eq!(id.soc.platform_id.as_deref(), Some("parrot"));
    }
}
