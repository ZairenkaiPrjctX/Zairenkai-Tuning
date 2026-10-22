// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Applying and restoring tuning operations.
//!
//! Adapted from Lynx `native/lynxd/src/apply/executor.rs` (`ProfileExecutor::execute`:
//! snapshot, validate, dry-run exit, lock, commit, verify, roll back) at commit
//! e56d6cd5005a543fa9798aa19b3627033593fbb0 (https://github.com/Zy0x/Lynx, GPL-3.0).
//! Changes (2026-10-22): one typed operation instead of a profile plan; validation against
//! the values the device advertises happens before the lock; the original value is saved in
//! a boot-scoped journal before the first write; any failed write or read-back rolls back
//! every target already written, and the rollback is itself read back; restore never
//! re-forces a value changed by someone else; results follow the contract (request status
//! apart from operation status, value read back per target).

use std::path::Path;

use serde::Serialize;
use ztune_core::blockdev;
use ztune_core::contract::{
    Error, ErrorCode, OperationStatus, RequestStatus, TargetOutcome, TargetResult,
};
use ztune_core::ops::{self, OperationId};

use crate::collect::{self, Target};
use crate::journal::{self, Journal, Lock};
use crate::sysroot::SysRoot;
use crate::writer;

#[derive(Debug, Clone, Serialize)]
pub struct RequestInfo {
    pub op: OperationId,
    pub value: String,
    pub dry_run: bool,
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationInfo {
    pub status: OperationStatus,
    pub targets: Vec<TargetResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub request: RequestInfo,
    pub operation: OperationInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

/// Normalizes what a node reads back to the plain current value.
pub fn read_back(op: OperationId) -> fn(&str) -> Option<String> {
    match op {
        OperationId::IoScheduler => |t| blockdev::parse_scheduler(t).map(|s| s.current),
        OperationId::GpuGovernor => |t| {
            let t = t.trim();
            (!t.is_empty()).then(|| t.to_string())
        },
    }
}

fn rejected(op: OperationId, value: &str, dry_run: bool, error: Error) -> Outcome {
    Outcome {
        request: RequestInfo {
            op,
            value: value.into(),
            dry_run,
            status: RequestStatus::Rejected,
        },
        operation: OperationInfo {
            status: OperationStatus::NotStarted,
            targets: vec![],
        },
        error: Some(error),
    }
}

fn result(t: &Target, value: &str, actual: Option<String>, outcome: TargetOutcome) -> TargetResult {
    TargetResult {
        target: t.name.clone(),
        path: t.path.clone(),
        before: Some(t.current.clone()),
        requested: value.into(),
        actual,
        outcome,
    }
}

/// Validates, then (unless `dry_run`) writes `value` to every target of `op`.
pub fn apply(
    root: &SysRoot,
    state_dir: &Path,
    op: OperationId,
    value: &str,
    dry_run: bool,
) -> Outcome {
    let targets = match collect::targets(root, op) {
        Ok(t) => t,
        Err(e) => return rejected(op, value, dry_run, e),
    };
    for t in &targets {
        if let Err(e) = ops::validate(op, value, &t.available) {
            let e = Error::new(e.code, format!("{}: {}", t.name, e.message));
            return rejected(op, value, dry_run, e);
        }
    }
    let accepted = |status, targets, error| Outcome {
        request: RequestInfo {
            op,
            value: value.into(),
            dry_run,
            status: RequestStatus::Accepted,
        },
        operation: OperationInfo { status, targets },
        error,
    };
    if dry_run {
        let rs = targets
            .iter()
            .map(|t| {
                let o = if t.current == value {
                    TargetOutcome::Unchanged
                } else {
                    TargetOutcome::WouldWrite
                };
                result(t, value, Some(t.current.clone()), o)
            })
            .collect();
        return accepted(OperationStatus::NotStarted, rs, None);
    }

    let boot = match journal::boot_id(root) {
        Ok(b) => b,
        Err(e) => return rejected(op, value, dry_run, e),
    };
    let _lock = match Lock::acquire(root, state_dir, &boot) {
        Ok(l) => l,
        Err(e) => return rejected(op, value, dry_run, e),
    };
    // Re-read under the lock: another writer may have run since validation.
    let targets = match collect::targets(root, op) {
        Ok(t) => t,
        Err(e) => return rejected(op, value, dry_run, e),
    };
    let mut jr = Journal::load(state_dir, &boot);
    let norm = read_back(op);
    let mut results: Vec<TargetResult> = Vec::new();
    let mut written: Vec<&Target> = Vec::new();
    let mut failure: Option<Error> = None;

    for t in &targets {
        if t.current == value {
            results.push(result(
                t,
                value,
                Some(t.current.clone()),
                TargetOutcome::Unchanged,
            ));
            continue;
        }
        jr.record(op, &t.name, &t.path, &t.current, value);
        if let Err(e) = jr.save(state_dir) {
            failure = Some(e);
            results.push(result(
                t,
                value,
                Some(t.current.clone()),
                TargetOutcome::NotAttempted,
            ));
            break;
        }
        match writer::write_verify(root, &t.path, value, norm) {
            Ok(v) if v.ok => {
                jr.mark_verified(&t.path);
                written.push(t);
                results.push(result(t, value, v.actual, TargetOutcome::Written));
            }
            Ok(v) => {
                failure = Some(Error::new(
                    ErrorCode::VerificationFailed,
                    format!(
                        "{} reads back {:?} after writing {value:?}",
                        t.path,
                        v.actual.as_deref().unwrap_or("<unreadable>")
                    ),
                ));
                written.push(t);
                results.push(result(
                    t,
                    value,
                    v.actual,
                    TargetOutcome::VerificationFailed,
                ));
                break;
            }
            Err(e) => {
                let actual = root.read_attr(&t.path).ok().and_then(|x| norm(&x));
                // Nothing changed: the journal entry is not needed.
                if actual.as_deref() == Some(t.current.as_str()) {
                    jr.remove(&t.path);
                } else {
                    written.push(t);
                }
                results.push(result(t, value, actual, TargetOutcome::WriteFailed));
                failure = Some(e);
                break;
            }
        }
    }

    let status = if failure.is_some() {
        let mut all_restored = true;
        for t in written.iter().rev() {
            let restored = writer::write_verify(root, &t.path, &t.current, norm);
            let (ok, actual) = match restored {
                Ok(v) => (v.ok, v.actual),
                Err(_) => (false, root.read_attr(&t.path).ok().and_then(|x| norm(&x))),
            };
            if ok {
                jr.remove(&t.path);
            }
            all_restored &= ok;
            if let Some(r) = results.iter_mut().find(|r| r.path == t.path) {
                r.actual = actual;
                r.outcome = if ok {
                    TargetOutcome::Restored
                } else {
                    TargetOutcome::RestoreFailed
                };
            }
        }
        let untouched: Vec<&Target> = targets
            .iter()
            .filter(|t| !results.iter().any(|r| r.path == t.path))
            .collect();
        for t in untouched {
            results.push(result(
                t,
                value,
                Some(t.current.clone()),
                TargetOutcome::NotAttempted,
            ));
        }
        if all_restored {
            OperationStatus::RolledBack
        } else {
            OperationStatus::Failed
        }
    } else if written.is_empty() {
        OperationStatus::NoChange
    } else {
        OperationStatus::AppliedVerified
    };
    if let Err(e) = jr.save(state_dir) {
        failure.get_or_insert(e);
    }
    accepted(status, results, failure)
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreOutcome {
    pub status: OperationStatus,
    pub targets: Vec<TargetResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

/// Restores every journalled node of this boot that still holds the value ztune wrote.
pub fn restore(root: &SysRoot, state_dir: &Path) -> RestoreOutcome {
    let fail = |e: Error| RestoreOutcome {
        status: OperationStatus::NotStarted,
        targets: vec![],
        error: Some(e),
    };
    let boot = match journal::boot_id(root) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let _lock = match Lock::acquire(root, state_dir, &boot) {
        Ok(l) => l,
        Err(e) => return fail(e),
    };
    let mut jr = Journal::load(state_dir, &boot);
    let mut results = Vec::new();
    let mut any_failed = false;
    let mut any_written = false;
    for e in jr.entries.clone() {
        let norm = read_back(e.op);
        let current = root.read_attr(&e.path).ok().and_then(|x| norm(&x));
        let mut r = TargetResult {
            target: e.target.clone(),
            path: e.path.clone(),
            before: current.clone(),
            requested: e.original.clone(),
            actual: current.clone(),
            outcome: TargetOutcome::Unchanged,
        };
        if current.as_deref() == Some(e.original.as_str()) {
            jr.remove(&e.path);
        } else if current.as_deref() != Some(e.applied.as_str()) {
            r.outcome = TargetOutcome::ChangedExternally;
            jr.remove(&e.path);
        } else {
            match writer::write_verify(root, &e.path, &e.original, norm) {
                Ok(v) if v.ok => {
                    any_written = true;
                    r.actual = v.actual;
                    r.outcome = TargetOutcome::Restored;
                    jr.remove(&e.path);
                }
                Ok(v) => {
                    any_failed = true;
                    r.actual = v.actual;
                    r.outcome = TargetOutcome::RestoreFailed;
                }
                Err(_) => {
                    any_failed = true;
                    r.outcome = TargetOutcome::RestoreFailed;
                }
            }
        }
        results.push(r);
    }
    let error = jr.save(state_dir).err();
    RestoreOutcome {
        status: if any_failed {
            OperationStatus::Failed
        } else if any_written {
            OperationStatus::AppliedVerified
        } else {
            OperationStatus::NoChange
        },
        targets: results,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture as common;
    use common::{device, get, put};

    fn sched(f: &common::Fixture, dev: &str) -> String {
        get(&f.root, &format!("/sys/block/{dev}/queue/scheduler"))
    }

    #[test]
    fn scheduler_applies_to_every_internal_device_and_restores() {
        let f = device("apply");
        let root = SysRoot::new(&f.root);
        let o = apply(&root, &f.state, OperationId::IoScheduler, "kyber", false);
        assert_eq!(o.request.status, RequestStatus::Accepted);
        assert_eq!(
            o.operation.status,
            OperationStatus::AppliedVerified,
            "{o:?}"
        );
        assert_eq!(o.operation.targets.len(), 2);
        assert!(o
            .operation
            .targets
            .iter()
            .all(|t| t.actual.as_deref() == Some("kyber")));
        assert_eq!(sched(&f, "sda"), "mq-deadline [kyber] none");
        // Removable and virtual devices are never touched.
        assert_eq!(sched(&f, "mmcblk1"), "[mq-deadline] none");
        assert_eq!(sched(&f, "loop0"), "[none] mq-deadline");
        // A second apply keeps the first original.
        apply(&root, &f.state, OperationId::IoScheduler, "none", false);
        let r = restore(&root, &f.state);
        assert_eq!(r.status, OperationStatus::AppliedVerified);
        assert_eq!(sched(&f, "sda"), "[mq-deadline] kyber none");
        assert_eq!(sched(&f, "sdb"), "[mq-deadline] kyber none");
        assert_eq!(restore(&root, &f.state).status, OperationStatus::NoChange);
    }

    #[test]
    fn failed_write_rolls_back_written_targets() {
        let f = device("rollback");
        put(&f.root, "/sys/block/sdb/queue/scheduler.fail", "");
        let root = SysRoot::new(&f.root);
        let o = apply(&root, &f.state, OperationId::IoScheduler, "kyber", false);
        assert_eq!(o.request.status, RequestStatus::Accepted);
        assert_eq!(o.operation.status, OperationStatus::RolledBack, "{o:?}");
        assert_eq!(o.error.as_ref().unwrap().code, ErrorCode::PermissionDenied);
        assert_eq!(o.operation.targets[0].outcome, TargetOutcome::Restored);
        assert_eq!(o.operation.targets[1].outcome, TargetOutcome::WriteFailed);
        assert_eq!(sched(&f, "sda"), "[mq-deadline] kyber none");
        assert!(
            Journal::load(&f.state, "6a1f0c58-0000-4000-8000-000000000001")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn validation_happens_before_any_write() {
        let f = device("validate");
        let root = SysRoot::new(&f.root);
        let o = apply(&root, &f.state, OperationId::IoScheduler, "bfq", false);
        assert_eq!(o.request.status, RequestStatus::Rejected);
        assert_eq!(o.operation.status, OperationStatus::NotStarted);
        assert_eq!(o.error.unwrap().code, ErrorCode::InvalidValue);
        let o = apply(
            &root,
            &f.state,
            OperationId::GpuGovernor,
            "userspace",
            false,
        );
        assert_eq!(o.error.unwrap().code, ErrorCode::Refused);
        assert!(!f.state.join("journal.json").exists());
        // A device that advertises the value on only some targets is refused as a whole.
        put(
            &f.root,
            "/sys/block/sdb/queue/scheduler",
            "[mq-deadline] none\n",
        );
        let o = apply(&root, &f.state, OperationId::IoScheduler, "kyber", false);
        assert_eq!(o.request.status, RequestStatus::Rejected);
        assert_eq!(sched(&f, "sda"), "[mq-deadline] kyber none");
    }

    #[test]
    fn dry_run_and_no_change_write_nothing() {
        let f = device("dry");
        let root = SysRoot::new(&f.root);
        let o = apply(&root, &f.state, OperationId::GpuGovernor, "powersave", true);
        assert_eq!(o.operation.status, OperationStatus::NotStarted);
        assert_eq!(o.operation.targets[0].outcome, TargetOutcome::WouldWrite);
        assert_eq!(
            get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
            "msm-adreno-tz"
        );
        let o = apply(
            &root,
            &f.state,
            OperationId::GpuGovernor,
            "msm-adreno-tz",
            false,
        );
        assert_eq!(o.operation.status, OperationStatus::NoChange);
    }

    #[test]
    fn restore_never_reforces_an_external_change() {
        let f = device("external");
        let root = SysRoot::new(&f.root);
        let o = apply(
            &root,
            &f.state,
            OperationId::GpuGovernor,
            "powersave",
            false,
        );
        assert_eq!(o.operation.status, OperationStatus::AppliedVerified);
        put(
            &f.root,
            "/sys/class/kgsl/kgsl-3d0/devfreq/governor",
            "performance\n",
        );
        let r = restore(&root, &f.state);
        assert_eq!(r.targets[0].outcome, TargetOutcome::ChangedExternally);
        assert_eq!(
            get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
            "performance"
        );
    }

    #[test]
    fn writes_need_a_boot_id() {
        let f = device("noboot");
        std::fs::remove_file(f.root.join("proc/sys/kernel/random/boot_id")).unwrap();
        let root = SysRoot::new(&f.root);
        let o = apply(
            &root,
            &f.state,
            OperationId::GpuGovernor,
            "powersave",
            false,
        );
        assert_eq!(o.request.status, RequestStatus::Rejected);
        assert_eq!(
            get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
            "msm-adreno-tz"
        );
    }
}
