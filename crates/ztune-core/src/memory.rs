// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Rve27 (Radika), Pavelc4 and the RvSystem-Monitor contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! `/proc/meminfo` parsing.
//!
//! Adapted from RvSystem-Monitor `rust/src/mm/memory.rs` at commit
//! b07fe27598555e49084bbcc40be558803286b9ef (https://github.com/Rve27/RvSystem-Monitor,
//! GPL-3.0). Changes (2026-10-22): values stay in bytes (upstream rounded to decimal GB);
//! a missing key is `None` instead of 0; parsing takes text instead of opening the file;
//! "zram" is reported as swap, because `SwapTotal` covers every swap device, not only zram.
//! The selected keys and the used = total - available derivation are upstream's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemInfo {
    pub total: Option<u64>,
    pub available: Option<u64>,
    pub cached: Option<u64>,
    pub buffers: Option<u64>,
    pub swap_total: Option<u64>,
    pub swap_free: Option<u64>,
}

impl MemInfo {
    pub fn used(&self) -> Option<u64> {
        self.total?.checked_sub(self.available?)
    }

    pub fn swap_used(&self) -> Option<u64> {
        self.swap_total?.checked_sub(self.swap_free?)
    }
}

/// Parses `/proc/meminfo`; every value there is in kB (KiB).
pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut m = MemInfo::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(kb) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
            continue;
        };
        if parts.next().is_some_and(|u| u != "kB") {
            continue;
        }
        let bytes = kb.checked_mul(1024);
        match key.trim() {
            "MemTotal" => m.total = bytes,
            "MemAvailable" => m.available = bytes,
            "Cached" => m.cached = bytes,
            "Buffers" => m.buffers = bytes,
            "SwapTotal" => m.swap_total = bytes,
            "SwapFree" => m.swap_free = bytes,
            _ => {}
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bytes() {
        let m = parse_meminfo(
            "MemTotal:        7812345 kB\nMemFree: 1 kB\nMemAvailable:    3906172 kB\n\
             Buffers: 10 kB\nCached: 20 kB\nSwapTotal: 4194300 kB\nSwapFree: 4194300 kB\n",
        );
        assert_eq!(m.total, Some(7_812_345 * 1024));
        assert_eq!(m.used(), Some((7_812_345 - 3_906_172) * 1024));
        assert_eq!(m.swap_used(), Some(0));
    }

    #[test]
    fn missing_keys_are_none() {
        let m = parse_meminfo("MemTotal: 100 kB\n");
        assert_eq!(m.available, None);
        assert_eq!(m.used(), None);
    }
}
