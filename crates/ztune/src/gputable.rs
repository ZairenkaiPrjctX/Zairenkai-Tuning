// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! `ztune gpu-table`: inspect, plan and stage a GPU frequency cap in a decompiled device tree.
//!
//! The editing logic is adapted from KonaBess (see `ztune_core::gpu_table`). The safety
//! gates are Zairenkai's: the command is never run by the module scripts (opt-in); the
//! device tree's `model` must equal the running device's; input is bounded and validated;
//! staging writes a byte-identical backup of the input next to the edited copy and needs a
//! confirmation token printed by `plan`. Nothing here reads, unpacks, repacks or flashes a
//! boot or vendor_boot partition: flashing is intentionally not implemented
//! (`docs/GPU_TABLE.md`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use ztune_core::contract::{Error, ErrorCode};
use ztune_core::gpu_table::{self, chip, BinSummary, CapPlan, Chip};

use crate::sysroot::SysRoot;

/// Largest device tree source accepted (decompiled DTBs are well under 4 MiB).
pub const MAX_DTS_BYTES: u64 = 16 * 1024 * 1024;
pub const DEVICE_MODEL_PATHS: &[&str] = &[
    "/sys/firmware/devicetree/base/model",
    "/proc/device-tree/model",
];

#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub dts: String,
    pub sha256: String,
    pub model: Option<String>,
    pub chip: Chip,
    pub single_bin: bool,
    pub max_table_levels: usize,
    pub bins: Vec<BinSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub dts: String,
    pub sha256: String,
    pub chip: Chip,
    pub device_model: String,
    pub edit: CapPlan,
    pub confirmation_token: String,
    pub flash: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Staged {
    pub plan: Plan,
    pub out_dir: String,
    pub backup: String,
    pub backup_sha256: String,
    pub modified: String,
    pub modified_sha256: String,
    pub verified: bool,
}

pub const FLASH_STATUS: &str =
    "not_implemented: Zairenkai does not unpack, repack or flash boot images";

fn read_dts(path: &Path) -> Result<(Vec<u8>, String), Error> {
    let meta = fs::metadata(path)
        .map_err(|e| Error::new(ErrorCode::InvalidValue, format!("{}: {e}", path.display())))?;
    if !meta.is_file() || meta.len() > MAX_DTS_BYTES {
        return Err(Error::new(
            ErrorCode::InvalidValue,
            format!(
                "{} is not a regular file of at most {MAX_DTS_BYTES} bytes",
                path.display()
            ),
        ));
    }
    let bytes = fs::read(path)
        .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        Error::new(
            ErrorCode::InvalidValue,
            format!(
                "{} is not UTF-8 device tree source (decompile the DTB first)",
                path.display()
            ),
        )
    })?;
    if text.contains('\0') || !text.contains("qcom,") {
        return Err(Error::new(
            ErrorCode::InvalidValue,
            format!(
                "{} does not look like Qualcomm device tree source",
                path.display()
            ),
        ));
    }
    Ok((bytes, text))
}

fn identify(text: &str) -> Result<Chip, Error> {
    chip::identify(text).ok_or_else(|| {
        Error::new(
            ErrorCode::Unsupported,
            "the device tree model is not a chip KonaBess supports",
        )
    })
}

pub fn inspect(path: &Path) -> Result<Inspection, Error> {
    let (bytes, text) = read_dts(path)?;
    let chip = identify(&text)?;
    let table = gpu_table::parse(&text, chip)?;
    Ok(Inspection {
        dts: path.display().to_string(),
        sha256: gpu_table::sha256_hex(&bytes),
        model: chip::model(&text),
        chip,
        single_bin: chip.single_bin(),
        max_table_levels: chip.max_table_levels(),
        bins: table.summary(),
    })
}

/// The running device's device tree model.
pub fn device_model(root: &SysRoot) -> Option<String> {
    DEVICE_MODEL_PATHS.iter().find_map(|p| {
        let b = root.read_bounded(p, 4096).ok()?;
        let s = String::from_utf8(b).ok()?;
        let s = s.trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    })
}

fn make_plan(root: &SysRoot, path: &Path, bin: u32, max_hz: u64) -> Result<(Plan, String), Error> {
    let (bytes, text) = read_dts(path)?;
    let chip = identify(&text)?;
    let dts_model = chip::model(&text).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidValue,
            "the device tree has no model property",
        )
    })?;
    let device = device_model(root).ok_or_else(|| {
        Error::new(
            ErrorCode::DeviceMismatch,
            "the running device's device tree model is unreadable; cannot verify the match",
        )
    })?;
    if device != dts_model {
        return Err(Error::new(
            ErrorCode::DeviceMismatch,
            format!("device tree model {dts_model:?} is not this device ({device:?})"),
        ));
    }
    let mut table = gpu_table::parse(&text, chip)?;
    let edit = table.cap_max_frequency(bin, max_hz)?;
    if edit.removed_hz.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidValue,
            format!("no level of bin {bin} is above {max_hz} Hz; nothing to change"),
        ));
    }
    let plan = Plan {
        dts: path.display().to_string(),
        sha256: gpu_table::sha256_hex(&bytes),
        chip,
        confirmation_token: gpu_table::confirmation_token(&bytes, bin, max_hz, &device),
        device_model: device,
        edit,
        flash: FLASH_STATUS,
    };
    Ok((plan, table.render()))
}

pub fn plan(root: &SysRoot, path: &Path, bin: u32, max_hz: u64) -> Result<Plan, Error> {
    make_plan(root, path, bin, max_hz).map(|(p, _)| p)
}

/// Writes `out/original.dts` (backup), `out/modified.dts` and `out/plan.json`, then reads
/// both back and re-parses the modified tree.
pub fn stage(
    root: &SysRoot,
    path: &Path,
    bin: u32,
    max_hz: u64,
    confirm: Option<&str>,
    out: &Path,
) -> Result<Staged, Error> {
    let (plan, modified) = make_plan(root, path, bin, max_hz)?;
    if confirm != Some(plan.confirmation_token.as_str()) {
        return Err(Error::new(
            ErrorCode::ConfirmationRequired,
            "run `ztune gpu-table plan` with the same arguments and pass its confirmation_token as --confirm",
        ));
    }
    if out.exists() {
        return Err(Error::new(
            ErrorCode::Refused,
            format!("{} exists; staging never overwrites", out.display()),
        ));
    }
    let io = |e: std::io::Error| Error::new(ErrorCode::Io, format!("{}: {e}", out.display()));
    fs::create_dir_all(out).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(out, fs::Permissions::from_mode(0o700)).map_err(io)?;
    }
    let input = fs::read(path).map_err(io)?;
    if gpu_table::sha256_hex(&input) != plan.sha256 {
        return Err(Error::new(
            ErrorCode::Refused,
            "the input changed while planning; nothing staged",
        ));
    }
    let backup: PathBuf = out.join("original.dts");
    let edited: PathBuf = out.join("modified.dts");
    fs::write(&backup, &input).map_err(io)?;
    fs::write(&edited, modified.as_bytes()).map_err(io)?;
    let backup_sha256 = gpu_table::sha256_hex(&fs::read(&backup).map_err(io)?);
    let modified_bytes = fs::read(&edited).map_err(io)?;
    let modified_sha256 = gpu_table::sha256_hex(&modified_bytes);
    let reparsed = String::from_utf8(modified_bytes)
        .ok()
        .and_then(|t| gpu_table::parse(&t, plan.chip).ok());
    let verified = backup_sha256 == plan.sha256
        && reparsed.is_some_and(|t| {
            t.summary()
                .iter()
                .find(|b| b.id == bin)
                .map(|b| b.levels.iter().map(|l| l.freq_hz).collect::<Vec<_>>())
                == Some(plan.edit.remaining_hz.clone())
        });
    let staged = Staged {
        out_dir: out.display().to_string(),
        backup: backup.display().to_string(),
        backup_sha256,
        modified: edited.display().to_string(),
        modified_sha256,
        verified,
        plan,
    };
    fs::write(
        out.join("plan.json"),
        serde_json::to_vec_pretty(&staged).expect("plan serializes"),
    )
    .map_err(io)?;
    if !verified {
        return Err(Error::new(
            ErrorCode::VerificationFailed,
            format!(
                "staged files in {} did not verify; do not use them",
                out.display()
            ),
        ));
    }
    Ok(staged)
}
