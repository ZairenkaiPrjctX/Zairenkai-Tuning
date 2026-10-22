// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Rve27 (Radika), Pavelc4 and the RvSystem-Monitor contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! CPU parsing: present cores, available frequencies and load from `/proc/stat`.
//!
//! Adapted from RvSystem-Monitor `rust/src/kernel/cpu.rs` at commit
//! b07fe27598555e49084bbcc40be558803286b9ef (https://github.com/Rve27/RvSystem-Monitor,
//! GPL-3.0). Changes (2026-10-22): split into pure functions over text so they can be tested
//! with fixtures; `present` returns core ids and parses comma lists of ranges (upstream
//! counted ranges as one core each); failed parses return `None` instead of 0 or "N/A";
//! the cached file descriptors and global load state were replaced by explicit samples;
//! the first sample of a core reports no load instead of -1. Paths and the load formula
//! (busy = total - idle - iowait over the interval) are upstream's.

/// cpufreq attribute paths tried in order for a core, as upstream does.
pub fn freq_paths(core: u32, names: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(names.len() * 2);
    for name in names {
        out.push(format!("/sys/devices/system/cpu/cpu{core}/cpufreq/{name}"));
        out.push(format!(
            "/sys/devices/system/cpu/cpufreq/policy{core}/{name}"
        ));
    }
    out
}

pub const CUR_FREQ: &[&str] = &["scaling_cur_freq", "cpuinfo_cur_freq"];
pub const MAX_FREQ: &[&str] = &["cpuinfo_max_freq", "scaling_max_freq"];
pub const MIN_FREQ: &[&str] = &["cpuinfo_min_freq", "scaling_min_freq"];
pub const GOVERNOR: &[&str] = &["scaling_governor"];
pub const AVAILABLE_FREQS: &[&str] = &["scaling_available_frequencies"];

/// Parses `/sys/devices/system/cpu/present` (`0-7`, `0-3,6`, `0`).
pub fn parse_present(text: &str) -> Option<Vec<u32>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut cores = Vec::new();
    for part in text.split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
                if b < a || b - a > 1023 {
                    return None;
                }
                cores.extend(a..=b);
            }
            None => cores.push(part.trim().parse().ok()?),
        }
    }
    cores.sort_unstable();
    cores.dedup();
    Some(cores)
}

pub fn parse_u64(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// Minimum and maximum of `scaling_available_frequencies`.
pub fn parse_available_freqs(text: &str) -> Option<(u64, u64)> {
    let mut vals: Vec<u64> = text
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    vals.sort_unstable();
    Some((*vals.first()?, *vals.last()?))
}

/// Jiffies of one `cpu` line of `/proc/stat`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CpuTicks {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTicks {
    pub fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    pub fn idle_total(&self) -> u64 {
        self.idle + self.iowait
    }
}

/// One `/proc/stat` sample: the aggregate line and each `cpuN` line present.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StatSample {
    pub total: Option<CpuTicks>,
    pub cores: Vec<(u32, CpuTicks)>,
}

pub fn parse_stat(text: &str) -> StatSample {
    let mut s = StatSample::default();
    for line in text.lines() {
        if !line.starts_with("cpu") {
            continue;
        }
        let mut it = line.split_whitespace();
        let Some(name) = it.next() else { continue };
        let mut field = || it.next().and_then(|v| v.parse::<u64>().ok());
        let (Some(user), Some(nice), Some(system), Some(idle)) =
            (field(), field(), field(), field())
        else {
            continue;
        };
        // Older kernels omit the trailing columns; missing ones count as zero jiffies.
        let ticks = CpuTicks {
            user,
            nice,
            system,
            idle,
            iowait: field().unwrap_or(0),
            irq: field().unwrap_or(0),
            softirq: field().unwrap_or(0),
            steal: field().unwrap_or(0),
        };
        if name == "cpu" {
            s.total = Some(ticks);
        } else if let Some(id) = name.strip_prefix("cpu").and_then(|n| n.parse().ok()) {
            s.cores.push((id, ticks));
        }
    }
    s
}

/// Busy percentage between two samples, or `None` when no time elapsed.
pub fn load_between(prev: CpuTicks, curr: CpuTicks) -> Option<f64> {
    let total = curr.total().checked_sub(prev.total())?;
    let idle = curr.idle_total().saturating_sub(prev.idle_total());
    if total == 0 {
        return None;
    }
    let busy = total.saturating_sub(idle) as f64 * 100.0 / total as f64;
    Some((busy * 10.0).round() / 10.0)
}

/// Load of `core` (`None` = aggregate) between two samples.
pub fn core_load(prev: &StatSample, curr: &StatSample, core: Option<u32>) -> Option<f64> {
    let pick = |s: &StatSample| match core {
        None => s.total,
        Some(id) => s.cores.iter().find(|(c, _)| *c == id).map(|(_, t)| *t),
    };
    load_between(pick(prev)?, pick(curr)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_lists() {
        assert_eq!(parse_present("0-7\n").unwrap(), (0..8).collect::<Vec<_>>());
        assert_eq!(parse_present("0-3,6").unwrap(), vec![0, 1, 2, 3, 6]);
        assert_eq!(parse_present("0").unwrap(), vec![0]);
        assert!(parse_present("").is_none());
        assert!(parse_present("3-1").is_none());
        assert!(parse_present("x").is_none());
    }

    #[test]
    fn available_freqs() {
        assert_eq!(
            parse_available_freqs("300000 1804800 576000 "),
            Some((300_000, 1_804_800))
        );
        assert_eq!(parse_available_freqs(""), None);
    }

    #[test]
    fn load_from_two_samples() {
        let a = parse_stat("cpu  100 0 100 800 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0\nintr 1\n");
        let b = parse_stat("cpu  150 0 150 900 0 0 0 0\ncpu0 100 0 50 450 0 0 0 0\n");
        // aggregate: total +200, idle +100 -> 50 %
        assert_eq!(core_load(&a, &b, None), Some(50.0));
        // cpu0: total +100, idle +50 -> 50 %
        assert_eq!(core_load(&a, &b, Some(0)), Some(50.0));
        assert_eq!(core_load(&a, &b, Some(1)), None);
        assert_eq!(core_load(&a, &a, None), None);
    }

    #[test]
    fn iowait_counts_as_idle() {
        let prev = CpuTicks::default();
        let curr = CpuTicks {
            user: 25,
            idle: 50,
            iowait: 25,
            ..Default::default()
        };
        assert_eq!(load_between(prev, curr), Some(25.0));
    }

    #[test]
    fn freq_paths_order() {
        let p = freq_paths(4, CUR_FREQ);
        assert_eq!(
            p[0],
            "/sys/devices/system/cpu/cpu4/cpufreq/scaling_cur_freq"
        );
        assert_eq!(
            p[1],
            "/sys/devices/system/cpu/cpufreq/policy4/scaling_cur_freq"
        );
    }
}
