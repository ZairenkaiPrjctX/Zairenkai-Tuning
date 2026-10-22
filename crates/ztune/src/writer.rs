// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Attribute writes with read-back verification.
//!
//! Adapted from Lynx `native/lynxd/src/sysfs/writer.rs` (`write`, `write_verify`) at commit
//! e56d6cd5005a543fa9798aa19b3627033593fbb0 (https://github.com/Zy0x/Lynx, GPL-3.0).
//! Changes (2026-10-22): writes go through the fixture-capable [`SysRoot`]; the
//! `ForcePermission` mode (chmod 0666 and retry) is removed: a denied write is reported,
//! never forced; the read-back comparison is supplied by the caller, because a scheduler
//! reads back as a bracketed list rather than the written word; errors map to the
//! contract's codes. The truncate-except-procfs rule and newline termination are
//! upstream's.

use std::io;

use ztune_core::contract::{Error, ErrorCode};

use crate::sysroot::SysRoot;

/// Maps a write error to the contract.
pub fn write_error(path: &str, e: &io::Error) -> Error {
    let code = match e.kind() {
        io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        io::ErrorKind::NotFound => ErrorCode::Unsupported,
        _ => ErrorCode::Io,
    };
    Error::new(code, format!("writing {path}: {e}"))
}

/// What a verified write left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The value read back, normalized by `read_back`.
    pub actual: Option<String>,
    pub ok: bool,
}

/// Writes `value`, reads the attribute back and normalizes it with `read_back` (which
/// returns the current value as a plain word), then compares.
pub fn write_verify(
    root: &SysRoot,
    path: &str,
    value: &str,
    read_back: impl Fn(&str) -> Option<String>,
) -> Result<Verified, Error> {
    root.write_attr(path, value)
        .map_err(|e| write_error(path, &e))?;
    let actual = root.read_attr(path).ok().and_then(|t| read_back(&t));
    Ok(Verified {
        ok: actual.as_deref() == Some(value),
        actual,
    })
}
