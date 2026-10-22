// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! The tuning operations of contract schema 1 and their validation.
//!
//! There is no free-form write: each operation names one kind of kernel attribute, and a
//! value is accepted only if it is well formed and the device itself advertises it.

use serde::{Deserialize, Serialize};

use crate::contract::{Error, ErrorCode};
use crate::gpu;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationId {
    /// Block I/O scheduler of every internal storage device (`queue/scheduler`).
    #[serde(rename = "io.scheduler")]
    IoScheduler,
    /// Governor of the GPU's devfreq node.
    #[serde(rename = "gpu.governor")]
    GpuGovernor,
}

impl OperationId {
    pub const ALL: [OperationId; 2] = [OperationId::IoScheduler, OperationId::GpuGovernor];

    pub fn name(self) -> &'static str {
        match self {
            OperationId::IoScheduler => "io.scheduler",
            OperationId::GpuGovernor => "gpu.governor",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.name() == s)
    }

    /// The capability that reports this operation.
    pub fn capability(self) -> &'static str {
        self.name()
    }
}

/// Kernel governor and scheduler names: 1-32 bytes of `[a-z0-9_-]`.
pub fn well_formed(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Validates `value` for `op` against what the device advertises.
pub fn validate(op: OperationId, value: &str, advertised: &[String]) -> Result<(), Error> {
    if !well_formed(value) {
        return Err(Error::new(
            ErrorCode::InvalidValue,
            format!("{value:?} is not a kernel name (1-32 of [a-z0-9_-])"),
        ));
    }
    if op == OperationId::GpuGovernor {
        if let Some(why) = gpu::refusal(value) {
            return Err(Error::new(ErrorCode::Refused, why));
        }
    }
    if !advertised.iter().any(|a| a == value) {
        return Err(Error::new(
            ErrorCode::InvalidValue,
            format!(
                "{value:?} is not advertised by the device for {} (advertised: {})",
                op.name(),
                advertised.join(" ")
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adv(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn names_round_trip() {
        for op in OperationId::ALL {
            assert_eq!(OperationId::parse(op.name()), Some(op));
            assert_eq!(
                serde_json::to_value(op).unwrap(),
                serde_json::Value::from(op.name())
            );
        }
        assert_eq!(OperationId::parse("cpu.max_freq"), None);
    }

    #[test]
    fn validation() {
        let a = adv(&["mq-deadline", "kyber", "none"]);
        assert!(validate(OperationId::IoScheduler, "kyber", &a).is_ok());
        assert_eq!(
            validate(OperationId::IoScheduler, "bfq", &a)
                .unwrap_err()
                .code,
            ErrorCode::InvalidValue
        );
        assert_eq!(
            validate(OperationId::IoScheduler, "x; rm -rf /", &a)
                .unwrap_err()
                .code,
            ErrorCode::InvalidValue
        );
        let g = adv(&["msm-adreno-tz", "userspace"]);
        assert_eq!(
            validate(OperationId::GpuGovernor, "userspace", &g)
                .unwrap_err()
                .code,
            ErrorCode::Refused
        );
    }
}
