// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! Test helpers over the synthetic Snapdragon-like fixture in `tests/fixtures`. Values are
//! invented for the tests, not captured from a device.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub struct Fixture {
    pub root: PathBuf,
    pub state: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.state);
    }
}

pub fn put(root: &Path, path: &str, content: &str) {
    let p = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

pub fn get(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path.trim_start_matches('/')))
        .unwrap()
        .trim()
        .to_string()
}

pub const MODEL: &str = "Qualcomm Technologies, Inc. kona v2.1 SoC";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

/// A private copy of `tests/fixtures/synthetic-kona` that the test may change.
pub fn device(name: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "ztune-fixture-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("root");
    let state = base.join("state");
    copy_tree(&fixtures().join("synthetic-kona"), &root);
    Fixture { root, state }
}

/// A decompiled-DTB-like source for the fixture's chip (multi-bin kona).
pub fn kona_dts() -> String {
    fs::read_to_string(fixtures().join("kona-synthetic.dts")).unwrap()
}
