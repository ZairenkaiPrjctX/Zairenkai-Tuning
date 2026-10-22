// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! The restore journal and the process lock.
//!
//! Before any write, the original value of the node is saved with the current boot id.
//! Kernel attributes reset at reboot, so a journal from an earlier boot is discarded, never
//! replayed. Entries keep the first original seen this boot, so repeated changes restore to
//! the value the device had before Zairenkai touched it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ztune_core::contract::{Error, ErrorCode};
use ztune_core::ops::OperationId;

use crate::sysroot::{write_atomic, SysRoot};

pub const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";
pub const JOURNAL_FILE: &str = "journal.json";
pub const LOCK_FILE: &str = "lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub op: OperationId,
    pub target: String,
    pub path: String,
    pub original: String,
    pub applied: String,
    /// The applied value was read back.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub format: u32,
    pub boot_id: String,
    pub entries: Vec<Entry>,
}

pub fn boot_id(root: &SysRoot) -> Result<String, Error> {
    match root.read_attr(BOOT_ID) {
        Ok(id) if !id.is_empty() => Ok(id),
        _ => Err(Error::new(
            ErrorCode::Io,
            format!("{BOOT_ID} is unreadable; writes are refused without a boot-scoped journal"),
        )),
    }
}

impl Journal {
    /// Loads the journal for `boot`; a missing, unreadable or earlier-boot journal is empty.
    pub fn load(state_dir: &Path, boot: &str) -> Journal {
        let fresh = Journal {
            format: 1,
            boot_id: boot.to_string(),
            entries: Vec::new(),
        };
        let Ok(text) = fs::read_to_string(state_dir.join(JOURNAL_FILE)) else {
            return fresh;
        };
        match serde_json::from_str::<Journal>(&text) {
            Ok(j) if j.format == 1 && j.boot_id == boot => j,
            _ => fresh,
        }
    }

    pub fn save(&self, state_dir: &Path) -> Result<(), Error> {
        let data = serde_json::to_vec_pretty(self).expect("journal serializes");
        write_atomic(&state_dir.join(JOURNAL_FILE), &data).map_err(|e| {
            Error::new(
                ErrorCode::Io,
                format!("saving {}: {e}", state_dir.join(JOURNAL_FILE).display()),
            )
        })
    }

    /// Records that `path` is about to change to `applied`, keeping the first original.
    pub fn record(
        &mut self,
        op: OperationId,
        target: &str,
        path: &str,
        original: &str,
        applied: &str,
    ) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.path == path) {
            e.applied = applied.to_string();
            e.verified = false;
        } else {
            self.entries.push(Entry {
                op,
                target: target.to_string(),
                path: path.to_string(),
                original: original.to_string(),
                applied: applied.to_string(),
                verified: false,
            });
        }
    }

    pub fn mark_verified(&mut self, path: &str) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.path == path) {
            e.verified = true;
        }
    }

    pub fn remove(&mut self, path: &str) {
        self.entries.retain(|e| e.path != path);
    }

    pub fn verified_for(&self, op: OperationId) -> bool {
        self.entries.iter().any(|e| e.op == op && e.verified)
    }
}

/// An exclusive lock on the state directory, released on drop.
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    /// Takes the lock. A lock left by a process that no longer runs (or by an earlier boot)
    /// is taken over.
    pub fn acquire(root: &SysRoot, state_dir: &Path, boot: &str) -> Result<Lock, Error> {
        fs::create_dir_all(state_dir).map_err(|e| {
            Error::new(
                ErrorCode::Io,
                format!("creating {}: {e}", state_dir.display()),
            )
        })?;
        let path = state_dir.join(LOCK_FILE);
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    let _ = writeln!(f, "{} {boot}", std::process::id());
                    return Ok(Lock { path });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let held = fs::read_to_string(&path).unwrap_or_default();
                    let mut it = held.split_whitespace();
                    let pid = it.next().unwrap_or("");
                    let same_boot = it.next() == Some(boot);
                    let alive = !pid.is_empty()
                        && pid.bytes().all(|b| b.is_ascii_digit())
                        && root.exists(&format!("/proc/{pid}"));
                    if same_boot && alive {
                        return Err(Error::new(
                            ErrorCode::Busy,
                            format!("another ztune (pid {pid}) holds {}", path.display()),
                        ));
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(e) => {
                    return Err(Error::new(
                        ErrorCode::Io,
                        format!("locking {}: {e}", path.display()),
                    ))
                }
            }
        }
        Err(Error::new(ErrorCode::Busy, "lock contended"))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ztune-journal-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn earlier_boot_is_discarded_and_first_original_kept() {
        let d = tmp("boot");
        let mut j = Journal::load(&d, "boot-a");
        j.record(
            OperationId::IoScheduler,
            "sda",
            "/sys/block/sda/queue/scheduler",
            "mq-deadline",
            "kyber",
        );
        j.record(
            OperationId::IoScheduler,
            "sda",
            "/sys/block/sda/queue/scheduler",
            "kyber",
            "none",
        );
        j.save(&d).unwrap();
        let again = Journal::load(&d, "boot-a");
        assert_eq!(again.entries.len(), 1);
        assert_eq!(again.entries[0].original, "mq-deadline");
        assert_eq!(again.entries[0].applied, "none");
        assert!(Journal::load(&d, "boot-b").entries.is_empty());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn lock_is_exclusive_and_stale_locks_are_taken_over() {
        let d = tmp("lock");
        let root_dir = tmp("lockroot");
        let root = SysRoot::new(&root_dir);
        // Our own pid "runs" in the fixture.
        fs::create_dir_all(root.path(&format!("/proc/{}", std::process::id()))).unwrap();
        let l = Lock::acquire(&root, &d, "b").unwrap();
        assert_eq!(
            Lock::acquire(&root, &d, "b").err().unwrap().code,
            ErrorCode::Busy
        );
        drop(l);
        fs::write(d.join(LOCK_FILE), "999999 b\n").unwrap();
        assert!(Lock::acquire(&root, &d, "b").is_ok());
        fs::remove_dir_all(&d).unwrap();
        fs::remove_dir_all(&root_dir).unwrap();
    }
}
