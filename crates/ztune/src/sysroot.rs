// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! File-system root. Every device read and write goes through a [`SysRoot`]: production
//! uses `/`, tests a fixture directory, so no test touches the host's `/sys` or `/proc`.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ztune_core::contract::DataStatus;

/// sysfs attributes are at most one page.
pub const MAX_ATTR_BYTES: u64 = 4096;
/// `/proc/stat` and `/proc/meminfo` can exceed a page on many-core devices.
pub const MAX_PROC_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysRoot {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub status: DataStatus,
    pub detail: String,
}

impl ReadError {
    pub fn from_io(path: &str, e: &io::Error) -> Self {
        let status = match e.kind() {
            io::ErrorKind::NotFound => DataStatus::Unsupported,
            io::ErrorKind::PermissionDenied => DataStatus::PermissionDenied,
            io::ErrorKind::InvalidData => DataStatus::ParseError,
            _ => DataStatus::Unavailable,
        };
        Self {
            status,
            detail: format!("{path}: {e}"),
        }
    }
}

impl SysRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Maps a device path (`/sys/...`) into the root.
    pub fn path(&self, device_path: &str) -> PathBuf {
        self.root.join(device_path.trim_start_matches('/'))
    }

    pub fn exists(&self, device_path: &str) -> bool {
        self.path(device_path).exists()
    }

    pub fn read_bounded(&self, device_path: &str, limit: u64) -> Result<Vec<u8>, ReadError> {
        let f = fs::File::open(self.path(device_path))
            .map_err(|e| ReadError::from_io(device_path, &e))?;
        let mut buf = Vec::new();
        f.take(limit + 1)
            .read_to_end(&mut buf)
            .map_err(|e| ReadError::from_io(device_path, &e))?;
        if buf.len() as u64 > limit {
            return Err(ReadError {
                status: DataStatus::ParseError,
                detail: format!("{device_path}: larger than {limit} bytes"),
            });
        }
        Ok(buf)
    }

    /// Reads a text attribute, trimmed.
    pub fn read_attr(&self, device_path: &str) -> Result<String, ReadError> {
        let b = self.read_bounded(device_path, MAX_ATTR_BYTES)?;
        String::from_utf8(b)
            .map(|s| s.trim().to_string())
            .map_err(|_| ReadError {
                status: DataStatus::ParseError,
                detail: format!("{device_path}: not UTF-8"),
            })
    }

    pub fn read_proc(&self, device_path: &str) -> Result<String, ReadError> {
        let b = self.read_bounded(device_path, MAX_PROC_BYTES)?;
        String::from_utf8(b).map_err(|_| ReadError {
            status: DataStatus::ParseError,
            detail: format!("{device_path}: not UTF-8"),
        })
    }

    /// Directory entry names, sorted, or empty if the directory does not exist.
    pub fn list(&self, device_path: &str) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.path(device_path))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Where a symlink in the tree points, as text (sysfs `/sys/block/*` entries).
    pub fn link_target(&self, device_path: &str) -> Option<String> {
        fs::read_link(self.path(device_path))
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Writes one attribute: open for writing (no create), write `value` and a newline.
    pub fn write_attr(&self, device_path: &str, value: &str) -> io::Result<()> {
        let p = self.path(device_path);
        let mut opts = fs::OpenOptions::new();
        opts.write(true);
        // procfs files do not support truncation; sysfs attributes ignore it.
        if !device_path.starts_with("/proc/") {
            opts.truncate(true);
        }
        #[cfg(test)]
        let value = &match test_kernel::write(&p, value)? {
            Some(rendered) => rendered,
            None => value.to_string(),
        };
        let mut f = opts.open(&p)?;
        f.write_all(format!("{value}\n").as_bytes())?;
        f.flush()
    }
}

/// Writes `data` to `path` through a temporary file and a rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Unit tests only: plain files do not behave like sysfs attributes, so a fixture can ask
/// for kernel semantics. A `queue/scheduler` file is rewritten as the kernel prints it
/// (EINVAL for an unknown name), and a sibling `<attr>.fail` file makes the write fail
/// with EACCES.
#[cfg(test)]
pub(crate) mod test_kernel {
    use std::io;
    use std::path::Path;

    pub fn write(p: &Path, value: &str) -> io::Result<Option<String>> {
        let mut fail = p.as_os_str().to_owned();
        fail.push(".fail");
        if Path::new(&fail).exists() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        if !p.ends_with("queue/scheduler") {
            return Ok(None);
        }
        let text = std::fs::read_to_string(p)?;
        let names: Vec<String> = text
            .split_whitespace()
            .map(|t| t.trim_matches(|c| c == '[' || c == ']').to_string())
            .collect();
        if !names.iter().any(|n| n == value) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Ok(Some(
            names
                .iter()
                .map(|n| {
                    if n == value {
                        format!("[{n}]")
                    } else {
                        n.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        ))
    }
}
