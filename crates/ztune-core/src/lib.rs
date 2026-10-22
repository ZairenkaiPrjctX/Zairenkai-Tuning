// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! Zairenkai Tuning core: the versioned contract, parsers and validation. No I/O.
//!
//! Parts of this crate are adapted from GPL-3.0 projects; each adapted file names its
//! upstream file, commit and authors in its header, and `docs/PROVENANCE.md` lists the
//! changes. This program is free software: you can redistribute it and/or modify it under
//! the terms of the GNU General Public License as published by the Free Software
//! Foundation, version 3. It is distributed WITHOUT ANY WARRANTY; see `LICENSE`.

pub mod blockdev;
pub mod contract;
pub mod cpu;
pub mod gpu;
pub mod gpu_table;
pub mod memory;
pub mod ops;
pub mod platform;
pub mod thermal;
