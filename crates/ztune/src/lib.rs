// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! `ztune`: telemetry and validated tuning for the Zairenkai root module.
//!
//! This program is free software: you can redistribute it and/or modify it under the terms
//! of the GNU General Public License as published by the Free Software Foundation,
//! version 3. It is distributed WITHOUT ANY WARRANTY; see `LICENSE`.

pub mod apply;
pub mod cli;
pub mod collect;
pub mod gputable;
pub mod journal;
pub mod sysroot;
pub mod writer;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_fixture;
