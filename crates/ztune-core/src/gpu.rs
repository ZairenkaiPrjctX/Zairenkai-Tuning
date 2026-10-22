// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! GPU devfreq: locating the GPU's devfreq node and its governors.
//!
//! Adapted from Lynx `native/lynxd/src/hardware/capability.rs` (`resolve_gpu`, KGSL paths)
//! and `native/lynxd/src/subsystems/gpu.rs` (`set_gov`) at commit
//! e56d6cd5005a543fa9798aa19b3627033593fbb0 (https://github.com/Zy0x/Lynx, GPL-3.0).
//! Changes (2026-10-22): a governor must be one the node advertises (upstream wrote any
//! string, and substituted a built-in list when none was advertised); `userspace` is
//! refused because it hands frequency control to a writer Zairenkai does not have; no
//! chmod fallback; generic devfreq nodes are used only when exactly one GPU-named node
//! exists. Frequency locking, MediaTek GED writes and boost writes were not adapted.

/// The Adreno KGSL devfreq directory, preferred when it exists.
pub const KGSL_DEVFREQ: &str = "/sys/class/kgsl/kgsl-3d0/devfreq";
pub const DEVFREQ_CLASS: &str = "/sys/class/devfreq";

/// Substrings of `/sys/class/devfreq` entry names that identify a GPU.
pub const GPU_NAME_HINTS: &[&str] = &["kgsl", "gpu", "mali", "g3d"];

/// Governors never set by Zairenkai, whatever the node advertises.
pub const REFUSED_GOVERNORS: &[(&str, &str)] = &[(
    "userspace",
    "userspace hands frequency control to a writer; Zairenkai writes no GPU frequency",
)];

pub fn parse_governors(text: &str) -> Vec<String> {
    let mut v: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    v.dedup();
    v
}

/// Picks the GPU devfreq entry among `/sys/class/devfreq` names: exactly one match or none.
pub fn pick_devfreq(names: &[String]) -> Result<Option<&String>, Vec<&String>> {
    let hits: Vec<&String> = names
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            GPU_NAME_HINTS.iter().any(|h| l.contains(h))
        })
        .collect();
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits[0])),
        _ => Err(hits),
    }
}

pub fn refusal(governor: &str) -> Option<&'static str> {
    REFUSED_GOVERNORS
        .iter()
        .find(|(g, _)| *g == governor)
        .map(|(_, why)| *why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governors() {
        assert_eq!(
            parse_governors("msm-adreno-tz performance powersave\n"),
            vec!["msm-adreno-tz", "performance", "powersave"]
        );
        assert!(refusal("userspace").is_some());
        assert!(refusal("msm-adreno-tz").is_none());
    }

    #[test]
    fn devfreq_pick() {
        let names: Vec<String> = ["soc:qcom,cpu-cpu-llcc-bw", "3d00000.qcom,kgsl-3d0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            pick_devfreq(&names).unwrap().unwrap(),
            "3d00000.qcom,kgsl-3d0"
        );
        let two: Vec<String> = vec!["13000000.mali".into(), "gpu-extra".into()];
        assert!(pick_devfreq(&two).is_err());
        assert_eq!(pick_devfreq(&[]).unwrap(), None);
    }
}
