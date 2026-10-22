// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Block I/O scheduler parsing and the internal-storage target set.
//!
//! Adapted from Lynx `native/lynxd/src/hardware/capability.rs` (`resolve_storage`, scheduler
//! token parsing) and `native/lynxd/src/subsystems/io.rs` (`set_scheduler` read-back check) at
//! commit e56d6cd5005a543fa9798aa19b3627033593fbb0 (https://github.com/Zy0x/Lynx, GPL-3.0).
//! Changes (2026-10-22): pure parsing; the read-back check is a function of the text read
//! back; a scheduler file without a bracketed current value is a parse error instead of an
//! empty name; targets are only non-virtual, non-removable devices (upstream also matched
//! name prefixes and SD cards); the read_ahead and MountX paths were not adapted.

/// Device name prefixes that are never storage targets (virtual or memory-backed).
pub const SKIP_PREFIXES: &[&str] = &["loop", "ram", "zram", "dm-", "md", "nbd", "sr"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduler {
    pub current: String,
    pub available: Vec<String>,
}

/// Parses `queue/scheduler`, e.g. `mq-deadline kyber [bfq] none`.
pub fn parse_scheduler(text: &str) -> Option<Scheduler> {
    let mut current = None;
    let mut available = Vec::new();
    for token in text.split_whitespace() {
        if let Some(name) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            if current.is_some() || name.is_empty() {
                return None;
            }
            current = Some(name.to_string());
            available.push(name.to_string());
        } else {
            available.push(token.to_string());
        }
    }
    // A single-queue device that prints only "none" has no choice to offer.
    Some(Scheduler {
        current: current?,
        available,
    })
}

/// True if the text read back from `queue/scheduler` shows `wanted` as current.
pub fn scheduler_is(text: &str, wanted: &str) -> bool {
    parse_scheduler(text).is_some_and(|s| s.current == wanted)
}

/// Whether a `/sys/block` entry can be a target, before reading its attributes.
pub fn candidate_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !SKIP_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let s = parse_scheduler("mq-deadline kyber [bfq] none\n").unwrap();
        assert_eq!(s.current, "bfq");
        assert_eq!(s.available, vec!["mq-deadline", "kyber", "bfq", "none"]);
        assert!(parse_scheduler("none").is_none());
        assert!(parse_scheduler("[a] [b]").is_none());
        assert!(parse_scheduler("[]").is_none());
    }

    #[test]
    fn read_back() {
        assert!(scheduler_is("[mq-deadline] none", "mq-deadline"));
        assert!(!scheduler_is("mq-deadline [none]", "mq-deadline"));
        assert!(!scheduler_is("garbage", "mq-deadline"));
    }

    #[test]
    fn names() {
        assert!(candidate_name("sda"));
        assert!(candidate_name("mmcblk0"));
        assert!(!candidate_name("loop7"));
        assert!(!candidate_name("zram0"));
        assert!(!candidate_name("dm-3"));
        assert!(!candidate_name("../x"));
    }
}
