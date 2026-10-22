// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! Loading device profiles: the set compiled into the binary (`profiles/index.json`) and,
//! for tests and maintainers, a directory given with `--profiles-dir`.
//!
//! Profiles are untrusted data and are validated before use. A profile marked `synthetic`
//! exists only for tests: it is rejected from the embedded set, and from a directory it is
//! honoured only when ztune runs against a fixture root (`--sysroot` other than `/`), so a
//! test profile can never authorize a write on a real device.

use std::fs;
use std::path::Path;

use serde::Serialize;
use ztune_core::contract::Error;
use ztune_core::profile::{self, Profile};

const EMBEDDED: &str = include_str!("../../../profiles/index.json");
/// Bounds for a profiles directory.
pub const MAX_PROFILE_BYTES: u64 = 256 * 1024;
pub const MAX_PROFILES: usize = 512;

#[derive(Debug, Clone, Serialize)]
pub struct Rejected {
    pub source: String,
    pub error: Error,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Loaded {
    pub profiles: Vec<Profile>,
    pub rejected: Vec<Rejected>,
}

fn reject(out: &mut Loaded, source: String, error: Error) {
    out.rejected.push(Rejected { source, error });
}

/// Loads every profile. `allow_synthetic` is true only in fixture mode.
pub fn load(dir: Option<&Path>, allow_synthetic: bool) -> Loaded {
    let mut out = Loaded::default();
    match serde_json::from_str::<Vec<serde_json::Value>>(EMBEDDED) {
        Ok(items) => {
            for (i, v) in items.into_iter().enumerate() {
                let src = format!("embedded[{i}]");
                match profile::parse(&v.to_string()) {
                    Ok(p) if p.synthetic => reject(
                        &mut out,
                        src,
                        Error::new(
                            ztune_core::contract::ErrorCode::Refused,
                            format!("{}: synthetic profiles are never embedded", p.profile_id),
                        ),
                    ),
                    Ok(p) => out.profiles.push(p),
                    Err(e) => reject(&mut out, src, e),
                }
            }
        }
        Err(e) => reject(
            &mut out,
            "embedded".into(),
            Error::new(ztune_core::contract::ErrorCode::InvalidValue, e.to_string()),
        ),
    }
    let Some(dir) = dir else { return out };
    let mut files: Vec<_> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    for path in files.into_iter().take(MAX_PROFILES) {
        let src = path.display().to_string();
        let text = match fs::metadata(&path) {
            Ok(m) if m.is_file() && m.len() <= MAX_PROFILE_BYTES => fs::read_to_string(&path),
            _ => {
                reject(
                    &mut out,
                    src,
                    Error::new(
                        ztune_core::contract::ErrorCode::InvalidValue,
                        format!("not a regular file of at most {MAX_PROFILE_BYTES} bytes"),
                    ),
                );
                continue;
            }
        };
        match text.map_err(|e| Error::new(ztune_core::contract::ErrorCode::Io, e.to_string())) {
            Ok(t) => match profile::parse(&t) {
                Ok(p) if p.synthetic && !allow_synthetic => reject(
                    &mut out,
                    src,
                    Error::new(
                        ztune_core::contract::ErrorCode::Refused,
                        format!("{}: synthetic profile outside a fixture root", p.profile_id),
                    ),
                ),
                Ok(p) => out.profiles.push(p),
                Err(e) => reject(&mut out, src, e),
            },
            Err(e) => reject(&mut out, src, e),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_set_is_valid_and_empty_until_a_device_is_validated() {
        let l = load(None, false);
        assert!(l.rejected.is_empty(), "{:?}", l.rejected);
        assert!(l.profiles.is_empty());
    }

    #[test]
    fn synthetic_profiles_only_in_fixture_mode() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/profiles");
        let real = load(Some(&dir), false);
        assert!(real.profiles.is_empty());
        assert!(!real.rejected.is_empty());
        let fixture = load(Some(&dir), true);
        assert!(!fixture.profiles.is_empty());
    }
}
